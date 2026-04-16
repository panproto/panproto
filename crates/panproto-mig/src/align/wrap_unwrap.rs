//! Wrap/unwrap alignment strategy.
//!
//! Detects the common pattern where one schema carries a group of
//! correlated leaves directly under a container (flat layout) and the
//! other nests them under an intermediate record vertex (wrapped
//! layout). Concretely, given source
//!
//! ```text
//! root --foo_x--> x : string
//! root --foo_y--> y : string
//! ```
//!
//! and target
//!
//! ```text
//! root --foo--> wrapper : object
//! wrapper --x--> x : string
//! wrapper --y--> y : string
//! ```
//!
//! the strategy proposes an anchor `(root, root)` plus anchors for the
//! leaves with confidence proportional to how many correlated leaves
//! match, and emits explanations tagged as a wrap/unwrap idiom so
//! downstream lens construction can realize the pairing via a
//! `HoistField` / `NestField` composition inside the existing
//! elementary-endofunctor vocabulary.
//!
//! Detection heuristics:
//!
//! * The "flat" side has at least two edges out of the same source
//!   vertex whose names share a common prefix separator (`_`, `-`, or a
//!   `camelCase` boundary).
//! * The "wrapped" side has a container vertex with an outgoing edge to
//!   an intermediate record whose own children's names match the
//!   suffixes of the flat side (e.g. `foo_x` → `foo/x`).
//!
//! The strategy emits anchors only when the prefix/suffix relation is
//! consistent across at least two correlated fields — a single match is
//! too weak to distinguish from coincidence.

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_schema::Schema;

use super::{Anchor, StrategyTag, kinds_compatible, token_similarity};

/// Emit anchors arising from wrap/unwrap pairings in either direction
/// (source flat ↔ target wrapped or source wrapped ↔ target flat).
#[must_use]
pub fn wrap_unwrap_anchors(src: &Schema, tgt: &Schema) -> Vec<Anchor> {
    let mut anchors = Vec::new();
    anchors.extend(detect_one_direction(src, tgt, false));
    anchors.extend(detect_one_direction(tgt, src, true));
    anchors
}

/// Detect wrap→unwrap pairings where `flat` holds the correlated
/// leaf edges and `wrapped` holds the intermediate record.
///
/// When `swap` is `true`, the schemas arrived reversed and emitted
/// anchors must be flipped so that `Anchor.src` always refers to the
/// caller's source side.
fn detect_one_direction(flat: &Schema, wrapped: &Schema, swap: bool) -> Vec<Anchor> {
    let mut out = Vec::new();

    for flat_parent in flat.vertices.keys() {
        let flat_edges = flat.outgoing_edges(flat_parent);
        if flat_edges.len() < 2 {
            continue;
        }

        // Group flat children by longest-common-prefix token (splitting
        // on '_', '-', or camelCase boundary).
        let groups = group_by_prefix(flat_edges);
        if groups.is_empty() {
            continue;
        }

        for (prefix, flat_group) in &groups {
            if flat_group.len() < 2 {
                continue;
            }

            // Look in wrapped for a parent with the same id or kind that
            // has an outgoing edge whose name equals `prefix` (or alias
            // of it via token similarity) leading to an intermediate
            // record with matching child names.
            let Some(candidate_parents) = candidate_wrap_parents(wrapped, flat_parent) else {
                continue;
            };

            for wrap_parent in &candidate_parents {
                // Find an edge named like the prefix.
                let Some(intermediate) = wrap_parent_intermediate(wrapped, wrap_parent, prefix)
                else {
                    continue;
                };

                // The intermediate vertex's children must cover the flat
                // suffixes with matching kinds.
                let matched =
                    match_wrapped_children(flat, flat_group, prefix, wrapped, &intermediate);
                if matched < 2 {
                    continue;
                }

                let denom = flat_group
                    .len()
                    .max(match_count_children(wrapped, &intermediate));
                #[allow(clippy::cast_precision_loss)]
                let coverage = matched as f64 / denom as f64;
                let confidence = 0.4f64.mul_add(coverage, 0.6).clamp(0.4, 0.95);

                // Parent ↔ parent anchor: flat_parent ↔ wrap_parent.
                if kinds_compatible(flat, flat_parent, wrapped, wrap_parent) {
                    push_anchor(
                        &mut out,
                        flat_parent,
                        wrap_parent,
                        confidence,
                        format!(
                            "wrap/unwrap pairing on {matched} field(s) under prefix '{prefix}': {} ↔ {}",
                            flat_parent.as_str(),
                            wrap_parent.as_str()
                        ),
                        swap,
                    );
                }
            }
        }
    }

    out
}

/// Push an anchor, swapping src/tgt when the caller's schemas were
/// reversed (so the emitted anchor always refers to src↔tgt in the
/// caller's frame).
fn push_anchor(
    out: &mut Vec<Anchor>,
    a: &Name,
    b: &Name,
    confidence: f64,
    explanation: String,
    swap: bool,
) {
    let (src, tgt) = if swap { (b, a) } else { (a, b) };
    out.push(Anchor {
        src: src.clone(),
        tgt: tgt.clone(),
        confidence,
        strategy: StrategyTag::WrapUnwrap,
        explanation,
    });
}

/// Group outgoing edges by the longest common prefix token in their
/// names. Returns a map `prefix → list of (suffix, edge_name, target)`.
fn group_by_prefix(edges: &[panproto_schema::Edge]) -> HashMap<String, Vec<(String, Name, Name)>> {
    let mut groups: HashMap<String, Vec<(String, Name, Name)>> = HashMap::new();
    for edge in edges {
        let Some(name) = edge.name.as_deref() else {
            continue;
        };
        let Some((prefix, suffix)) = split_prefix_suffix(name) else {
            continue;
        };
        groups.entry(prefix.to_owned()).or_default().push((
            suffix.to_owned(),
            Name::from(name),
            edge.tgt.clone(),
        ));
    }
    groups
}

/// Split `identifier` into `(prefix, suffix)` at the earliest
/// non-trailing separator. Returns `None` when there is no split.
///
/// Accepts `_`, `-`, or a `camelCase` boundary. Always returns the
/// leftmost split so that `foo_bar_baz` groups under prefix `foo`.
fn split_prefix_suffix(name: &str) -> Option<(&str, &str)> {
    let bytes = name.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if i == 0 || i == bytes.len().saturating_sub(1) {
            continue;
        }
        if b == b'_' || b == b'-' {
            return Some((&name[..i], &name[i + 1..]));
        }
    }
    // camelCase boundary: first lowercase-to-uppercase transition.
    let chars: Vec<char> = name.chars().collect();
    for (i, _) in chars.iter().enumerate().skip(1) {
        if chars[i - 1].is_lowercase() && chars[i].is_uppercase() {
            // Boundary before index `i`.
            let byte_idx: usize = chars[..i].iter().map(|c| c.len_utf8()).sum();
            return Some((&name[..byte_idx], &name[byte_idx..]));
        }
    }
    None
}

/// Pick candidate wrapper-side parents for `flat_parent`.
///
/// Accepts any wrapper vertex whose id equals `flat_parent`; if none
/// does, falls back to token-similar names so cross-namespace wraps
/// (e.g. `post` vs `post.body`) still register. The caller checks kind
/// compatibility before emitting anchors.
fn candidate_wrap_parents(wrapped: &Schema, flat_parent: &Name) -> Option<Vec<Name>> {
    let mut out: Vec<Name> = wrapped
        .vertices
        .keys()
        .filter(|id| id.as_str() == flat_parent.as_str())
        .cloned()
        .collect();

    if out.is_empty() {
        for id in wrapped.vertices.keys() {
            if token_similarity(id.as_str(), flat_parent.as_str()) > 0.6 {
                out.push(id.clone());
            }
        }
    }

    if out.is_empty() { None } else { Some(out) }
}

/// Locate the intermediate record vertex below `wrap_parent` reached
/// via an edge whose name equals the flat prefix (up to alias/casing).
fn wrap_parent_intermediate(wrapped: &Schema, wrap_parent: &Name, prefix: &str) -> Option<Name> {
    for edge in wrapped.outgoing_edges(wrap_parent) {
        let Some(name) = edge.name.as_deref() else {
            continue;
        };
        if name.eq_ignore_ascii_case(prefix) || token_similarity(name, prefix) > 0.85 {
            return Some(edge.tgt.clone());
        }
    }
    None
}

/// Count how many of `flat_group`'s suffixes find a kind-compatible
/// counterpart among `intermediate`'s outgoing edge names.
fn match_wrapped_children(
    flat: &Schema,
    flat_group: &[(String, Name, Name)],
    _prefix: &str,
    wrapped: &Schema,
    intermediate: &Name,
) -> usize {
    let wrapped_children: Vec<(&str, Name)> = wrapped
        .outgoing_edges(intermediate)
        .iter()
        .filter_map(|e| e.name.as_deref().map(|n| (n, e.tgt.clone())))
        .collect();

    let mut matched = 0usize;
    for (suffix, _flat_edge_name, flat_leaf) in flat_group {
        let Some((_, wrap_leaf)) = wrapped_children
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(suffix) || token_similarity(n, suffix) > 0.8)
        else {
            continue;
        };
        if kinds_compatible(flat, flat_leaf, wrapped, wrap_leaf) {
            matched += 1;
        }
    }
    matched
}

/// Number of named outgoing edges from `vertex` in `schema`.
fn match_count_children(schema: &Schema, vertex: &Name) -> usize {
    schema
        .outgoing_edges(vertex)
        .iter()
        .filter(|e| e.name.is_some())
        .count()
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
    fn split_prefix_suffix_on_underscore() {
        assert_eq!(split_prefix_suffix("foo_bar"), Some(("foo", "bar")));
        assert_eq!(split_prefix_suffix("subject_uri"), Some(("subject", "uri")));
        assert_eq!(split_prefix_suffix("a_b_c"), Some(("a", "b_c")));
        assert_eq!(split_prefix_suffix("nosep"), None);
    }

    #[test]
    fn split_prefix_suffix_on_camel() {
        assert_eq!(split_prefix_suffix("fooBar"), Some(("foo", "Bar")));
        assert_eq!(split_prefix_suffix("subjectUri"), Some(("subject", "Uri")));
    }

    #[test]
    fn detects_flat_to_wrapped_pairing() {
        // Flat source: root has `subject_uri` and `subject_cid` leaves.
        // Wrapped target: root has `subject` → wrapper → `uri`, `cid`.
        let flat = build_schema(
            &[
                ("root", "object"),
                ("root.uri", "string"),
                ("root.cid", "string"),
            ],
            &[
                ("root", "root.uri", "prop", "subject_uri"),
                ("root", "root.cid", "prop", "subject_cid"),
            ],
        );
        let wrapped = build_schema(
            &[
                ("root", "object"),
                ("root.subject", "object"),
                ("root.subject.uri", "string"),
                ("root.subject.cid", "string"),
            ],
            &[
                ("root", "root.subject", "prop", "subject"),
                ("root.subject", "root.subject.uri", "prop", "uri"),
                ("root.subject", "root.subject.cid", "prop", "cid"),
            ],
        );

        let anchors = wrap_unwrap_anchors(&flat, &wrapped);
        assert!(
            !anchors.is_empty(),
            "wrap/unwrap strategy should detect the pairing"
        );
        assert!(
            anchors
                .iter()
                .any(|a| a.src.as_str() == "root" && a.tgt.as_str() == "root"),
            "expected root↔root anchor; got {anchors:?}"
        );
        // Explanation mentions wrap/unwrap and the prefix.
        let exp = &anchors
            .iter()
            .find(|a| a.src.as_str() == "root")
            .unwrap()
            .explanation;
        assert!(exp.contains("wrap/unwrap"), "explanation: {exp}");
        assert!(
            exp.contains("subject"),
            "explanation must name the prefix: {exp}"
        );
    }

    #[test]
    fn detects_wrapped_to_flat_pairing_via_swap() {
        // Swap the sides: source wrapped, target flat.
        let wrapped = build_schema(
            &[
                ("root", "object"),
                ("root.subject", "object"),
                ("root.subject.uri", "string"),
                ("root.subject.cid", "string"),
            ],
            &[
                ("root", "root.subject", "prop", "subject"),
                ("root.subject", "root.subject.uri", "prop", "uri"),
                ("root.subject", "root.subject.cid", "prop", "cid"),
            ],
        );
        let flat = build_schema(
            &[
                ("root", "object"),
                ("root.uri", "string"),
                ("root.cid", "string"),
            ],
            &[
                ("root", "root.uri", "prop", "subject_uri"),
                ("root", "root.cid", "prop", "subject_cid"),
            ],
        );

        let anchors = wrap_unwrap_anchors(&wrapped, &flat);
        assert!(
            anchors
                .iter()
                .any(|a| a.src.as_str() == "root" && a.tgt.as_str() == "root"),
            "swap direction should still anchor root↔root; got {anchors:?}"
        );
    }

    #[test]
    fn does_not_fire_on_unrelated_fields() {
        // Flat source has two unrelated leaves with no common prefix;
        // should not produce anchors.
        let flat = build_schema(
            &[
                ("root", "object"),
                ("root.x", "string"),
                ("root.y", "string"),
            ],
            &[
                ("root", "root.x", "prop", "unrelated_a"),
                ("root", "root.y", "prop", "different_b"),
            ],
        );
        let wrapped = build_schema(
            &[("root", "object"), ("root.text", "string")],
            &[("root", "root.text", "prop", "text")],
        );
        let anchors = wrap_unwrap_anchors(&flat, &wrapped);
        // No common prefix on flat side can find a matching wrapper.
        assert!(
            anchors
                .iter()
                .all(|a| a.strategy != StrategyTag::WrapUnwrap || a.confidence < 0.4),
            "should not emit spurious wrap/unwrap anchors: {anchors:?}"
        );
    }

    #[test]
    fn requires_at_least_two_correlated_fields() {
        // Only a single `subject_uri` on flat side — not enough evidence.
        let flat = build_schema(
            &[("root", "object"), ("root.uri", "string")],
            &[("root", "root.uri", "prop", "subject_uri")],
        );
        let wrapped = build_schema(
            &[
                ("root", "object"),
                ("root.subject", "object"),
                ("root.subject.uri", "string"),
            ],
            &[
                ("root", "root.subject", "prop", "subject"),
                ("root.subject", "root.subject.uri", "prop", "uri"),
            ],
        );
        let anchors = wrap_unwrap_anchors(&flat, &wrapped);
        assert!(
            !anchors
                .iter()
                .any(|a| a.strategy == StrategyTag::WrapUnwrap),
            "single correlated field is insufficient"
        );
    }
}
