//! Suffix-equality alignment strategy.
//!
//! Proposes an anchor `(s, t)` whenever the terminal dot-separated
//! segment of `s`'s vertex ID equals the terminal segment of `t`'s and
//! the kinds are compatible. Fills the gap where two schemas share
//! user-visible prop or field names but live under disjoint namespace
//! prefixes (e.g. `x.y.z.tags` against `p.q.r.tags`).
//!
//! Runs at every stringency tier: the CSP's naturality check validates
//! every proposal, and the downside of a false-positive suffix collision
//! is a little extra work for the solver rather than an accepted bad
//! mapping.
//!
//! Skipped when the full IDs already match; [`super::exact_anchors`]
//! handles that case with [`super::StrategyTag::Exact`], which has
//! strictly higher priority than [`super::StrategyTag::ExactSuffix`].

use std::collections::HashMap;

use panproto_schema::Schema;

use super::{Anchor, StrategyTag, kinds_and_constraints_compatible};

/// Emit anchors for every `(source, target)` pair whose vertex IDs
/// share a terminal dot-segment and whose kinds are compatible.
///
/// Confidence is `1.0` because a terminal-segment equality is a
/// categorical name match at the local-prop level; the prefix is
/// namespacing metadata, not schema content.
#[must_use]
pub fn suffix_anchors(src: &Schema, tgt: &Schema) -> Vec<Anchor> {
    let mut src_ids: Vec<&panproto_gat::Name> = src.vertices.keys().collect();
    src_ids.sort_by(|a, b| a.as_str().cmp(b.as_str()));

    let mut tgt_ids: Vec<&panproto_gat::Name> = tgt.vertices.keys().collect();
    tgt_ids.sort_by(|a, b| a.as_str().cmp(b.as_str()));

    let mut tgt_by_tail: HashMap<&str, Vec<&panproto_gat::Name>> = HashMap::new();
    for tgt_id in &tgt_ids {
        let tail = terminal_segment(tgt_id.as_str());
        tgt_by_tail.entry(tail).or_default().push(tgt_id);
    }

    let mut out = Vec::new();
    for src_id in src_ids {
        let src_s = src_id.as_str();
        if !src_s.contains('.') {
            // No prefix to strip: the exact strategy already proposes
            // an anchor for any bare-name match on the target side.
            continue;
        }
        let tail = terminal_segment(src_s);
        let Some(tgts) = tgt_by_tail.get(tail) else {
            continue;
        };
        for tgt_id in tgts {
            if src_id.as_str() == tgt_id.as_str() {
                // Exact-id match: leave it to exact_anchors so the
                // Exact tag wins the priority tiebreak.
                continue;
            }
            if !kinds_and_constraints_compatible(src, src_id, tgt, tgt_id) {
                continue;
            }
            out.push(Anchor {
                src: (*src_id).clone(),
                tgt: (*tgt_id).clone(),
                confidence: 1.0,
                strategy: StrategyTag::ExactSuffix,
                explanation: format!(
                    "suffix match on '.{tail}': {} against {}",
                    src_id.as_str(),
                    tgt_id.as_str(),
                ),
            });
        }
    }
    out
}

/// Return the terminal segment of a dot-separated identifier. An input
/// with no dot returns the whole string.
fn terminal_segment(id: &str) -> &str {
    id.rsplit_once('.').map_or(id, |(_, tail)| tail)
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
    fn cross_namespace_tail_matches_produce_anchor() {
        // Different namespace prefixes, same trailing prop name,
        // identical kind: suffix strategy emits one anchor.
        let src = schema(&[("alpha.beta.gamma.tags", "object")]);
        let tgt = schema(&[("delta.epsilon.zeta.tags", "object")]);
        let anchors = suffix_anchors(&src, &tgt);
        assert_eq!(anchors.len(), 1);
        assert_eq!(anchors[0].src.as_str(), "alpha.beta.gamma.tags");
        assert_eq!(anchors[0].tgt.as_str(), "delta.epsilon.zeta.tags");
        assert!((anchors[0].confidence - 1.0).abs() < f64::EPSILON);
        assert_eq!(anchors[0].strategy, StrategyTag::ExactSuffix);
    }

    #[test]
    fn no_tail_match_returns_empty() {
        let src = schema(&[("a.b.foo", "string")]);
        let tgt = schema(&[("c.d.bar", "string")]);
        assert!(suffix_anchors(&src, &tgt).is_empty());
    }

    #[test]
    fn tail_match_with_incompatible_kinds_skipped() {
        let src = schema(&[("a.b.tags", "object")]);
        let tgt = schema(&[("c.d.tags", "string")]);
        assert!(suffix_anchors(&src, &tgt).is_empty());
    }

    #[test]
    fn full_id_match_delegates_to_exact_strategy() {
        let src = schema(&[("a.b.tags", "object")]);
        let tgt = schema(&[("a.b.tags", "object")]);
        // suffix_anchors skips this pair so the exact strategy owns it.
        assert!(suffix_anchors(&src, &tgt).is_empty());
    }

    #[test]
    fn bare_name_source_skipped() {
        // A source vertex with no dot in its id has no prefix to strip;
        // the exact strategy already handles any target named the same.
        let src = schema(&[("tags", "object")]);
        let tgt = schema(&[("a.b.tags", "object")]);
        assert!(suffix_anchors(&src, &tgt).is_empty());
    }

    #[test]
    fn multiple_targets_same_tail_all_emitted() {
        // Every target sharing the source's tail gets an anchor; the
        // CSP sorts out which one actually satisfies naturality.
        let src = schema(&[("a.b.foo", "string")]);
        let tgt = schema(&[
            ("x.y.foo", "string"),
            ("p.q.foo", "string"),
            ("r.s.bar", "string"),
        ]);
        let anchors = suffix_anchors(&src, &tgt);
        let targets: Vec<&str> = anchors.iter().map(|a| a.tgt.as_str()).collect();
        assert_eq!(anchors.len(), 2);
        assert!(targets.contains(&"x.y.foo"));
        assert!(targets.contains(&"p.q.foo"));
        assert!(!targets.contains(&"r.s.bar"));
    }

    #[test]
    fn deterministic_emission_order() {
        // Insertion order permuted on both sides; suffix_anchors must
        // produce the same ordered Vec across runs so the CSP's seed
        // merge is reproducible.
        let src1 = schema(&[
            ("a.b.foo", "string"),
            ("a.b.bar", "string"),
            ("a.b.baz", "string"),
        ]);
        let src2 = schema(&[
            ("a.b.baz", "string"),
            ("a.b.foo", "string"),
            ("a.b.bar", "string"),
        ]);
        let tgt = schema(&[
            ("x.y.foo", "string"),
            ("x.y.bar", "string"),
            ("x.y.baz", "string"),
        ]);
        let r1: Vec<_> = suffix_anchors(&src1, &tgt)
            .iter()
            .map(|a| (a.src.as_str().to_owned(), a.tgt.as_str().to_owned()))
            .collect();
        let r2: Vec<_> = suffix_anchors(&src2, &tgt)
            .iter()
            .map(|a| (a.src.as_str().to_owned(), a.tgt.as_str().to_owned()))
            .collect();
        assert_eq!(r1, r2);
    }

    #[test]
    fn multi_prop_cross_namespace_recovery() {
        // Two schemas under disjoint namespace prefixes share some
        // local prop names and not others; suffix matching should
        // recover exactly the shared-name anchors and leave the rest
        // unmatched.
        let src = schema(&[
            ("alpha.one.tags", "object"),
            ("alpha.one.text", "string"),
            ("alpha.one.labels", "object"),
            ("alpha.one.createdAt", "string"),
        ]);
        let tgt = schema(&[
            ("gamma.two.tags", "object"),
            ("gamma.two.labels", "object"),
            ("gamma.two.title", "string"),
            ("gamma.two.path", "string"),
        ]);
        let anchors = suffix_anchors(&src, &tgt);
        let pairs: Vec<(&str, &str)> = anchors
            .iter()
            .map(|a| (a.src.as_str(), a.tgt.as_str()))
            .collect();
        assert!(pairs.contains(&("alpha.one.tags", "gamma.two.tags")));
        assert!(pairs.contains(&("alpha.one.labels", "gamma.two.labels")));
        // `text` and `createdAt` have no matching-tailed target.
        assert!(!pairs.iter().any(|(s, _)| *s == "alpha.one.text"));
        assert!(!pairs.iter().any(|(s, _)| *s == "alpha.one.createdAt"));
    }

    #[test]
    fn terminal_segment_handles_no_dot() {
        assert_eq!(terminal_segment("tags"), "tags");
        assert_eq!(terminal_segment("a.b.c"), "c");
        assert_eq!(terminal_segment(""), "");
    }
}
