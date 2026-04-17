//! Exact-name-equality alignment strategy.
//!
//! Proposes an anchor `(s, t)` whenever `s` and `t` have identical names
//! and compatible kinds. Confidence is `1.0`.

use panproto_schema::Schema;

use super::{Anchor, StrategyTag, kinds_compatible};

/// Emit anchors for every source vertex whose name exists in `tgt` with
/// the same kind.
#[must_use]
pub fn exact_anchors(src: &Schema, tgt: &Schema) -> Vec<Anchor> {
    let mut src_ids: Vec<&panproto_gat::Name> = src.vertices.keys().collect();
    src_ids.sort_by(|a, b| a.as_str().cmp(b.as_str()));

    let mut out = Vec::new();
    for src_id in src_ids {
        if tgt.has_vertex(src_id) && kinds_compatible(src, src_id, tgt, src_id) {
            out.push(Anchor {
                src: src_id.clone(),
                tgt: src_id.clone(),
                confidence: 1.0,
                strategy: StrategyTag::Exact,
                explanation: format!("exact name match: {}", src_id.as_str()),
            });
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use panproto_schema::{Protocol, SchemaBuilder};

    fn test_protocol() -> Protocol {
        Protocol {
            name: "test".into(),
            schema_theory: "ThTest".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec!["object".into(), "string".into()],
            constraint_sorts: vec![],
            ..Protocol::default()
        }
    }

    fn schema(verts: &[(&str, &str)]) -> Schema {
        let proto = test_protocol();
        let mut b = SchemaBuilder::new(&proto);
        for (id, k) in verts {
            b = b.vertex(id, k, None::<&str>).unwrap();
        }
        b.build().unwrap()
    }

    #[test]
    fn disjoint_minimal_schema_returns_empty() {
        // Smallest schemas with no shared vertex names: no panic, no anchors.
        let src = schema(&[("unique_s", "string")]);
        let tgt = schema(&[("unique_t", "string")]);
        assert!(exact_anchors(&src, &tgt).is_empty());
    }

    #[test]
    fn deterministic_order_across_runs() {
        // Insertion order permuted on source side; output must be stable when sorted.
        let s1 = schema(&[("a", "string"), ("b", "string"), ("c", "string")]);
        let s2 = schema(&[("c", "string"), ("a", "string"), ("b", "string")]);
        let t = schema(&[("a", "string"), ("b", "string"), ("c", "string")]);
        let r1: Vec<_> = exact_anchors(&s1, &t)
            .iter()
            .map(|a| a.src.as_str().to_owned())
            .collect();
        let r2: Vec<_> = exact_anchors(&s2, &t)
            .iter()
            .map(|a| a.src.as_str().to_owned())
            .collect();
        assert_eq!(r1, vec!["a", "b", "c"]);
        assert_eq!(r1, r2, "emission order must be deterministic");
    }

    #[test]
    fn every_anchor_has_compatible_kinds() {
        let src = schema(&[("a", "string"), ("b", "object")]);
        let tgt = schema(&[("a", "string"), ("b", "string")]);
        for anchor in exact_anchors(&src, &tgt) {
            assert!(kinds_compatible(&src, &anchor.src, &tgt, &anchor.tgt));
        }
    }

    #[test]
    fn single_isolated_vertex_schema() {
        // Schema-building requires ≥ 1 vertex; smallest legal input is a
        // single isolated vertex. Guards against panics on the thinnest
        // legal schema.
        let s = schema(&[("only", "string")]);
        assert!(exact_anchors(&s, &s).len() == 1);
    }

    #[test]
    fn bit_identical_across_100_runs() {
        let s = schema(&[("a", "string"), ("b", "object"), ("c", "string")]);
        let t = schema(&[("a", "string"), ("b", "object"), ("c", "string")]);
        let baseline: Vec<(String, String, u64)> = exact_anchors(&s, &t)
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
            let again: Vec<(String, String, u64)> = exact_anchors(&s, &t)
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
    fn at_most_one_anchor_per_src_tgt_pair() {
        // With 5 vertices sharing the same kind on each side, exact_anchors
        // must still emit exactly one anchor per name and never duplicate.
        let verts: [(&str, &str); 5] = [
            ("a", "string"),
            ("b", "string"),
            ("c", "string"),
            ("d", "string"),
            ("e", "string"),
        ];
        let src = schema(&verts);
        let tgt = schema(&verts);
        let anchors = exact_anchors(&src, &tgt);
        let mut pairs: Vec<(String, String)> = anchors
            .iter()
            .map(|a| (a.src.as_str().into(), a.tgt.as_str().into()))
            .collect();
        pairs.sort();
        let deduped = {
            let mut p = pairs.clone();
            p.dedup();
            p
        };
        assert_eq!(pairs, deduped, "no duplicate (src, tgt) anchors");
        assert_eq!(pairs.len(), 5);
    }

    #[test]
    fn emits_anchors_only_for_shared_names_with_matching_kind() {
        let src = schema(&[("a", "string"), ("b", "object"), ("c", "string")]);
        let tgt = schema(&[("a", "string"), ("b", "string"), ("d", "string")]);

        let anchors = exact_anchors(&src, &tgt);
        let pairs: Vec<_> = anchors
            .iter()
            .map(|a| (a.src.as_str().to_owned(), a.tgt.as_str().to_owned()))
            .collect();

        assert!(pairs.contains(&("a".into(), "a".into())));
        // "b" exists on both sides but kinds differ → excluded.
        assert!(!pairs.iter().any(|(s, _)| s == "b"));
        // "c" missing on target → excluded.
        assert!(!pairs.iter().any(|(s, _)| s == "c"));
    }
}
