//! Description-token similarity alignment strategy.
//!
//! Extracts a per-vertex description string (the value of any constraint
//! whose sort is `description`) and cross-scores pairs by token bag
//! Jaccard plus character n-gram cosine, reusing the normalization
//! from [`super::token_similarity`]. Emits anchors above `threshold`.
//!
//! Protocols that do not carry descriptions produce no anchors: the
//! strategy silently no-ops on any schema whose vertices lack the
//! `description` constraint.
//!
//! Confidence equals the similarity score. The CSP's naturality check
//! validates every proposal.

use panproto_gat::Name;
use panproto_schema::Schema;

use super::{
    Anchor, StrategyTag, kinds_compatible,
    token_similarity::{char_ngram_cosine, token_jaccard, tokenize},
};

/// Constraint sort used to carry per-vertex descriptions. Protocols
/// that annotate descriptions attach a constraint with this sort;
/// protocols that don't leave it absent, which surfaces here as zero
/// anchors.
const DESCRIPTION_SORT: &str = "description";

/// Retrieve the description string for a vertex, if any.
///
/// Returns the value of the first constraint on `vertex_id` whose sort
/// equals [`DESCRIPTION_SORT`]. `None` when the vertex carries no
/// description constraint (the common case in protocols that do not
/// annotate descriptions).
fn vertex_description<'a>(schema: &'a Schema, vertex_id: &Name) -> Option<&'a str> {
    schema
        .constraints
        .get(vertex_id)?
        .iter()
        .find(|c| c.sort.as_str() == DESCRIPTION_SORT)
        .map(|c| c.value.as_str())
}

/// Compound description similarity: `0.6 * Jaccard(tokens) + 0.4 *
/// cosine(bigrams)`, with an exact-string shortcut to `1.0`.
///
/// Uses the same tokenizer and n-gram machinery as
/// [`super::token_similarity::token_similarity`] so that behavior is
/// consistent across strategies. Diverges in that it does not fall
/// back to the cosine-only branch: description text is longer than
/// identifier text, and a pure character-overlap spike (two
/// descriptions that happen to share punctuation) would produce a lot
/// of spurious anchors on identifier-free text.
#[must_use]
pub fn description_similarity(a: &str, b: &str) -> f64 {
    if a == b {
        return 1.0;
    }
    let ta = tokenize(a);
    let tb = tokenize(b);
    let jac = token_jaccard(&ta, &tb);
    let cos = char_ngram_cosine(a, b, 2);
    0.6f64.mul_add(jac, 0.4 * cos).clamp(0.0, 1.0)
}

/// Emit description-similarity anchors.
///
/// For every source vertex with a non-empty description, find the
/// best-scoring kind-and-constraints-compatible target vertex that
/// also has a non-empty description. Emit an anchor if the score
/// exceeds `threshold`.
///
/// Skips exact-identifier matches (covered by the exact strategy) and
/// emits nothing when neither side carries descriptions.
#[must_use]
pub fn description_anchors(src: &Schema, tgt: &Schema, threshold: f64) -> Vec<Anchor> {
    let mut out = Vec::new();
    let mut src_ids: Vec<&Name> = src.vertices.keys().collect();
    src_ids.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    let mut tgt_ids: Vec<&Name> = tgt.vertices.keys().collect();
    tgt_ids.sort_by(|a, b| a.as_str().cmp(b.as_str()));

    for src_id in src_ids.iter().copied() {
        let Some(src_desc) = vertex_description(src, src_id) else {
            continue;
        };
        if src_desc.is_empty() {
            continue;
        }
        let mut best: Option<(Name, f64)> = None;
        for tgt_id in tgt_ids.iter().copied() {
            if src_id.as_str() == tgt_id.as_str() {
                continue;
            }
            // `kinds_compatible` rather than the stricter
            // `kinds_and_constraints_compatible` because the
            // description string itself lives in a constraint; any
            // two distinct-but-similar descriptions would fail the
            // strict test before ever reaching the similarity scorer.
            if !kinds_compatible(src, src_id, tgt, tgt_id) {
                continue;
            }
            let Some(tgt_desc) = vertex_description(tgt, tgt_id) else {
                continue;
            };
            if tgt_desc.is_empty() {
                continue;
            }
            let score = description_similarity(src_desc, tgt_desc);
            if best.as_ref().is_none_or(|(_, bs)| score > *bs) {
                best = Some((tgt_id.clone(), score));
            }
        }
        if let Some((tgt_id, score)) = best
            && score >= threshold
        {
            out.push(Anchor {
                src: src_id.clone(),
                tgt: tgt_id.clone(),
                confidence: score,
                strategy: StrategyTag::DescriptionSimilarity,
                explanation: format!(
                    "description similarity {:.2}: {} ↔ {}",
                    score,
                    src_id.as_str(),
                    tgt_id.as_str()
                ),
            });
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;
    use panproto_schema::{Protocol, SchemaBuilder};

    fn proto() -> Protocol {
        Protocol {
            name: "t".into(),
            schema_theory: "ThTest".into(),
            instance_theory: "ThWType".into(),
            edge_rules: vec![],
            obj_kinds: vec!["string".into(), "object".into()],
            constraint_sorts: vec!["description".into()],
            ..Protocol::default()
        }
    }

    fn schema(verts: &[(&str, &str, Option<&str>)]) -> panproto_schema::Schema {
        let p = proto();
        let mut b = SchemaBuilder::new(&p);
        for (id, kind, desc) in verts {
            b = b.vertex(id, kind, None::<&str>).unwrap();
            if let Some(d) = desc {
                b = b.constraint(id, "description", d);
            }
        }
        b.build().unwrap()
    }

    #[test]
    fn similar_descriptions_above_threshold_produce_anchor() {
        let src = schema(&[("a", "string", Some("The creation timestamp of the record"))]);
        let tgt = schema(&[(
            "b",
            "string",
            Some("The creation timestamp of the record entry"),
        )]);
        let anchors = description_anchors(&src, &tgt, 0.55);
        assert_eq!(anchors.len(), 1);
        assert_eq!(anchors[0].src.as_str(), "a");
        assert_eq!(anchors[0].tgt.as_str(), "b");
        assert_eq!(anchors[0].strategy, StrategyTag::DescriptionSimilarity);
    }

    #[test]
    fn dissimilar_descriptions_below_threshold_produce_nothing() {
        let src = schema(&[("a", "string", Some("Opaque cryptographic hash digest"))]);
        let tgt = schema(&[("b", "string", Some("Human-readable display title"))]);
        assert!(description_anchors(&src, &tgt, 0.55).is_empty());
    }

    #[test]
    fn empty_description_skipped() {
        let src = schema(&[("a", "string", Some(""))]);
        let tgt = schema(&[("b", "string", Some(""))]);
        assert!(description_anchors(&src, &tgt, 0.55).is_empty());
    }

    #[test]
    fn no_descriptions_yields_empty() {
        let src = schema(&[("a", "string", None)]);
        let tgt = schema(&[("b", "string", None)]);
        assert!(description_anchors(&src, &tgt, 0.55).is_empty());
    }

    #[test]
    fn exact_identifier_match_skipped() {
        // The exact strategy owns same-name anchors; description
        // strategy skips them so the priority table stays clean.
        let src = schema(&[("a", "string", Some("desc"))]);
        let tgt = schema(&[("a", "string", Some("desc"))]);
        assert!(description_anchors(&src, &tgt, 0.1).is_empty());
    }

    #[test]
    fn incompatible_kinds_skipped() {
        let src = schema(&[("a", "string", Some("text"))]);
        let tgt = schema(&[("b", "object", Some("text"))]);
        assert!(description_anchors(&src, &tgt, 0.1).is_empty());
    }

    #[test]
    fn paraphrased_descriptions_above_threshold() {
        // Rephrased text sharing most content tokens should clear 0.55.
        let src = schema(&[(
            "a",
            "string",
            Some("unique identifier for the user account"),
        )]);
        let tgt = schema(&[("b", "string", Some("unique identifier of the user account"))]);
        let anchors = description_anchors(&src, &tgt, 0.55);
        assert_eq!(anchors.len(), 1);
        assert!(anchors[0].confidence >= 0.55);
    }

    #[test]
    fn deterministic_across_permutations() {
        let s1 = schema(&[
            ("a", "string", Some("first name of the user")),
            ("b", "string", Some("last name of the user")),
        ]);
        let s2 = schema(&[
            ("b", "string", Some("last name of the user")),
            ("a", "string", Some("first name of the user")),
        ]);
        let t = schema(&[
            ("x", "string", Some("first name of the user account")),
            ("y", "string", Some("last name of the user account")),
        ]);
        let go = |s: &panproto_schema::Schema| {
            let mut r: Vec<_> = description_anchors(s, &t, 0.5)
                .iter()
                .map(|a| (a.src.as_str().to_owned(), a.tgt.as_str().to_owned()))
                .collect();
            r.sort();
            r
        };
        assert_eq!(go(&s1), go(&s2));
    }
}
