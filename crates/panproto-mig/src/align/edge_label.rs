//! Edge-label alignment strategy.
//!
//! Proposes an anchor `(e_src.tgt, e_tgt.tgt)` for every pair of edges
//! `(e_src, e_tgt)` where:
//!
//! * `e_src.name == e_tgt.name` (the optional human-readable label set on
//!   the fourth argument of `SchemaBuilder::edge`),
//! * `e_src.kind == e_tgt.kind` (the edge kind, e.g. `"prop"`),
//! * [`kinds_compatible`] on the child vertices.
//!
//! Complementary to [`super::suffix_anchors`]: edge-label targets
//! children reached via labeled edges from any parent, while suffix
//! targets vertex IDs whose terminal dot-segment happens to coincide.
//! On schemas whose path structure does not flow through labeled edges
//! the two strategies expose different anchor sets.
//!
//! Confidence is `1.0`: two child vertices reached by
//! same-name-same-kind edges are a structural name match at the local
//! label level. The CSP's naturality check validates every proposal.

use std::collections::{HashMap, HashSet};

use panproto_gat::Name;
use panproto_schema::Schema;

use super::evidence::Provenance;
use super::{Anchor, StrategyTag, kinds_and_constraints_compatible, kinds_compatible};

/// Emit anchors for every `(src_child, tgt_child)` pair reached by a
/// labeled edge of the same name and kind on each side, when the child
/// vertex kinds are compatible.
///
/// Multi-edge same-label cases (one vertex has two `prop` edges named
/// `id`) are fanned out: every label-matched target child is proposed,
/// then deduplicated at the `(src_child, tgt_child)` level so each
/// pair is emitted at most once.
#[must_use]
pub fn edge_label_anchors(src: &Schema, tgt: &Schema) -> Vec<Anchor> {
    // Group target edges by (label, kind) for O(|src_edges|) scan.
    let mut tgt_by_label_kind: HashMap<(&str, &str), Vec<&panproto_schema::Edge>> = HashMap::new();
    let mut tgt_edges: Vec<&panproto_schema::Edge> = tgt.edges.keys().collect();
    tgt_edges.sort();
    for edge in tgt_edges {
        let Some(label) = edge.name.as_deref() else {
            continue;
        };
        tgt_by_label_kind
            .entry((label, edge.kind.as_str()))
            .or_default()
            .push(edge);
    }

    let mut src_edges: Vec<&panproto_schema::Edge> = src.edges.keys().collect();
    src_edges.sort();

    let mut seen: HashSet<(Name, Name)> = HashSet::new();
    let mut out = Vec::new();
    for src_edge in src_edges {
        let Some(src_label) = src_edge.name.as_deref() else {
            continue;
        };
        let key = (src_label, src_edge.kind.as_str());
        let Some(candidates) = tgt_by_label_kind.get(&key) else {
            continue;
        };
        for tgt_edge in candidates {
            // Cheap gate first: compatible parent kinds. An edge named
            // `id` on a `record` vs one on a `list` are unlikely to
            // encode the same concept; reject the pair before the more
            // expensive child-side constraint check.
            if !kinds_compatible(src, &src_edge.src, tgt, &tgt_edge.src) {
                continue;
            }
            if !kinds_and_constraints_compatible(src, &src_edge.tgt, tgt, &tgt_edge.tgt) {
                continue;
            }
            let pair = (src_edge.tgt.clone(), tgt_edge.tgt.clone());
            if !seen.insert(pair.clone()) {
                continue;
            }
            out.push(Anchor {
                src: pair.0,
                tgt: pair.1,
                confidence: 1.0,
                strategy: StrategyTag::EdgeLabel,
                provenance: Provenance::DeclaredEdgeLabel,
                explanation: format!(
                    "edge-label '{}' ({}): {} child {} against {} child {}",
                    src_label,
                    src_edge.kind.as_str(),
                    src_edge.src.as_str(),
                    src_edge.tgt.as_str(),
                    tgt_edge.src.as_str(),
                    tgt_edge.tgt.as_str(),
                ),
            });
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use panproto_schema::{EdgeRule, Protocol, SchemaBuilder};

    fn test_protocol() -> Protocol {
        Protocol {
            name: "test".into(),
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
    fn cross_namespace_edge_label_matches_produce_anchor() {
        let proto = test_protocol();
        let src = SchemaBuilder::new(&proto)
            .vertex("alpha.parent", "object", None::<&str>)
            .unwrap()
            .vertex("alpha.child", "string", None::<&str>)
            .unwrap()
            .edge("alpha.parent", "alpha.child", "prop", Some("id"))
            .unwrap()
            .build()
            .unwrap();
        let tgt = SchemaBuilder::new(&proto)
            .vertex("beta.parent", "object", None::<&str>)
            .unwrap()
            .vertex("beta.child", "string", None::<&str>)
            .unwrap()
            .edge("beta.parent", "beta.child", "prop", Some("id"))
            .unwrap()
            .build()
            .unwrap();
        let anchors = edge_label_anchors(&src, &tgt);
        assert_eq!(anchors.len(), 1);
        assert_eq!(anchors[0].src.as_str(), "alpha.child");
        assert_eq!(anchors[0].tgt.as_str(), "beta.child");
        assert_eq!(anchors[0].strategy, StrategyTag::EdgeLabel);
        assert!((anchors[0].confidence - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn no_label_edges_emit_no_anchors() {
        let proto = test_protocol();
        let src = SchemaBuilder::new(&proto)
            .vertex("p", "object", None::<&str>)
            .unwrap()
            .vertex("c", "string", None::<&str>)
            .unwrap()
            .edge("p", "c", "prop", None)
            .unwrap()
            .build()
            .unwrap();
        let tgt = SchemaBuilder::new(&proto)
            .vertex("q", "object", None::<&str>)
            .unwrap()
            .vertex("d", "string", None::<&str>)
            .unwrap()
            .edge("q", "d", "prop", None)
            .unwrap()
            .build()
            .unwrap();
        assert!(edge_label_anchors(&src, &tgt).is_empty());
    }

    #[test]
    fn mismatched_child_kinds_skipped() {
        let proto = test_protocol();
        let src = SchemaBuilder::new(&proto)
            .vertex("p", "object", None::<&str>)
            .unwrap()
            .vertex("c", "string", None::<&str>)
            .unwrap()
            .edge("p", "c", "prop", Some("id"))
            .unwrap()
            .build()
            .unwrap();
        let tgt = SchemaBuilder::new(&proto)
            .vertex("q", "object", None::<&str>)
            .unwrap()
            .vertex("d", "object", None::<&str>)
            .unwrap()
            .edge("q", "d", "prop", Some("id"))
            .unwrap()
            .build()
            .unwrap();
        assert!(edge_label_anchors(&src, &tgt).is_empty());
    }

    #[test]
    fn label_match_but_different_kind_skipped() {
        // Two distinct edge kinds that happen to share a label: the
        // strategy keys on (label, kind), so these must not cross-match.
        let proto = Protocol {
            name: "t".into(),
            schema_theory: "ThTest".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![
                EdgeRule {
                    edge_kind: "prop".into(),
                    src_kinds: vec!["object".into()],
                    tgt_kinds: vec!["string".into()],
                },
                EdgeRule {
                    edge_kind: "item".into(),
                    src_kinds: vec!["object".into()],
                    tgt_kinds: vec!["string".into()],
                },
            ],
            obj_kinds: vec!["object".into(), "string".into()],
            constraint_sorts: vec![],
            ..Protocol::default()
        };
        let src = SchemaBuilder::new(&proto)
            .vertex("p", "object", None::<&str>)
            .unwrap()
            .vertex("c", "string", None::<&str>)
            .unwrap()
            .edge("p", "c", "prop", Some("id"))
            .unwrap()
            .build()
            .unwrap();
        let tgt = SchemaBuilder::new(&proto)
            .vertex("q", "object", None::<&str>)
            .unwrap()
            .vertex("d", "string", None::<&str>)
            .unwrap()
            .edge("q", "d", "item", Some("id"))
            .unwrap()
            .build()
            .unwrap();
        assert!(edge_label_anchors(&src, &tgt).is_empty());
    }

    #[test]
    fn multi_edge_same_label_fans_out() {
        // A target with two `prop` edges labeled `id` (pointing to
        // different child vertices) produces one anchor per distinct
        // child pair.
        let proto = test_protocol();
        let src = SchemaBuilder::new(&proto)
            .vertex("p", "object", None::<&str>)
            .unwrap()
            .vertex("c", "string", None::<&str>)
            .unwrap()
            .edge("p", "c", "prop", Some("id"))
            .unwrap()
            .build()
            .unwrap();
        let tgt = SchemaBuilder::new(&proto)
            .vertex("q1", "object", None::<&str>)
            .unwrap()
            .vertex("q2", "object", None::<&str>)
            .unwrap()
            .vertex("d1", "string", None::<&str>)
            .unwrap()
            .vertex("d2", "string", None::<&str>)
            .unwrap()
            .edge("q1", "d1", "prop", Some("id"))
            .unwrap()
            .edge("q2", "d2", "prop", Some("id"))
            .unwrap()
            .build()
            .unwrap();
        let anchors = edge_label_anchors(&src, &tgt);
        assert_eq!(anchors.len(), 2);
        let tgts: Vec<&str> = anchors.iter().map(|a| a.tgt.as_str()).collect();
        assert!(tgts.contains(&"d1"));
        assert!(tgts.contains(&"d2"));
    }

    #[test]
    fn deduplicates_same_child_pair_across_edges() {
        // Two source edges with the same label both point at the same
        // child, and likewise on the target side. The (child, child)
        // pair must be emitted only once.
        let proto = test_protocol();
        let src = SchemaBuilder::new(&proto)
            .vertex("p1", "object", None::<&str>)
            .unwrap()
            .vertex("p2", "object", None::<&str>)
            .unwrap()
            .vertex("c", "string", None::<&str>)
            .unwrap()
            .edge("p1", "c", "prop", Some("id"))
            .unwrap()
            .edge("p2", "c", "prop", Some("id"))
            .unwrap()
            .build()
            .unwrap();
        let tgt = SchemaBuilder::new(&proto)
            .vertex("q1", "object", None::<&str>)
            .unwrap()
            .vertex("q2", "object", None::<&str>)
            .unwrap()
            .vertex("d", "string", None::<&str>)
            .unwrap()
            .edge("q1", "d", "prop", Some("id"))
            .unwrap()
            .edge("q2", "d", "prop", Some("id"))
            .unwrap()
            .build()
            .unwrap();
        let anchors = edge_label_anchors(&src, &tgt);
        assert_eq!(anchors.len(), 1);
        assert_eq!(anchors[0].src.as_str(), "c");
        assert_eq!(anchors[0].tgt.as_str(), "d");
    }

    #[test]
    fn deterministic_across_insertion_orders() {
        let proto = test_protocol();
        let build = |pairs: &[(&str, &str, &str)]| {
            let mut b = SchemaBuilder::new(&proto)
                .vertex("p", "object", None::<&str>)
                .unwrap();
            for (child, _, _) in pairs {
                b = b.vertex(child, "string", None::<&str>).unwrap();
            }
            for (child, kind, label) in pairs {
                b = b.edge("p", child, kind, Some(*label)).unwrap();
            }
            b.build().unwrap()
        };
        let src1 = build(&[("a", "prop", "id"), ("b", "prop", "name")]);
        let src2 = build(&[("b", "prop", "name"), ("a", "prop", "id")]);
        let tgt = {
            let mut b = SchemaBuilder::new(&proto)
                .vertex("q", "object", None::<&str>)
                .unwrap()
                .vertex("x", "string", None::<&str>)
                .unwrap()
                .vertex("y", "string", None::<&str>)
                .unwrap();
            b = b.edge("q", "x", "prop", Some("id")).unwrap();
            b = b.edge("q", "y", "prop", Some("name")).unwrap();
            b.build().unwrap()
        };
        let r1: Vec<_> = edge_label_anchors(&src1, &tgt)
            .iter()
            .map(|a| (a.src.as_str().to_owned(), a.tgt.as_str().to_owned()))
            .collect();
        let r2: Vec<_> = edge_label_anchors(&src2, &tgt)
            .iter()
            .map(|a| (a.src.as_str().to_owned(), a.tgt.as_str().to_owned()))
            .collect();
        assert_eq!(r1, r2);
    }
}
