//! The reference quality score, kept unchanged so the decomposition can be
//! checked against it.
//!
//! [`reference_quality`] is the quality score the morphism search has always
//! computed: a weighted mean of four components (vertex name similarity, edge
//! name preservation, outgoing-edge-name Jaccard overlap, out-degree agreement)
//! accumulated in `f64` over a whole morphism. It is reproduced here exactly as
//! it was, down to the sort that fixes the summation order and the per-component
//! denominators, because its value is that it is the *untouched* thing.
//!
//! # Why it exists
//!
//! The search minimises a decomposed objective: the same four components, split
//! into one unary cost function per source vertex and one binary cost function
//! per source vertex pair, with every denominator fixed by the source schema
//! alone and every term rounded to fixed point once. That decomposition is
//! claimed to agree with this function on total morphisms whenever the
//! Jaccard component's two normalisers coincide, and to dominate it otherwise.
//! A claim of that shape is only worth stating if it is checkable, and it is
//! checkable only against an implementation that no one is free to adjust when
//! the check fails. This module is that implementation.
//!
//! Two consequences follow, and both are load bearing.
//!
//! First, **this is not on the search path**. Nothing in `solve` calls it. The
//! search never accumulates a score over a candidate assignment; it sums integer
//! cost function entries. A caller who wants the quality of an assignment reads
//! it back out of the integer objective.
//!
//! Second, **this is the only place `f64` accumulation survives** in the crate.
//! Every other float in the objective is converted to fixed point once, per cost
//! function entry, while the network is being built. The reasons are in the
//! [`cost`](crate::solve::cost) module docs; the short form is that a float sum
//! depends on the order it was taken in, and the order the search would take it
//! in depends on heuristics and hash iteration order.

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_schema::{Edge, Schema};

/// Compute a quality score for a morphism.
///
/// Higher is better. Four components:
/// 1. **Name similarity** (0.25): 1.0 - (avg edit distance / max name length)
/// 2. **Edge name preservation** (0.25): fraction of edges with matching names
/// 3. **Property-name Jaccard** (0.3): for each mapped vertex pair, Jaccard
///    similarity of their outgoing edge names, rewarding structural alignment
/// 4. **Degree similarity** (0.2): penalizes mappings where vertex degrees
///    differ significantly
///
/// `weights` is `[name, edge, prop, degree]`, the order
/// [`CostWeights::as_array`](crate::solve::CostWeights::as_array) produces.
///
/// This is the reference implementation of the objective, hidden from the
/// rendered documentation because no caller should reach for it: it exists so
/// that the decomposition the search actually minimises is a testable statement
/// rather than a claim. The module docs say what may and may not be changed
/// about it.
#[doc(hidden)]
#[must_use]
pub fn reference_quality(
    vertex_map: &HashMap<Name, Name>,
    edge_map: &HashMap<Edge, Edge>,
    src: &Schema,
    tgt: &Schema,
    weights: [f64; 4],
) -> f64 {
    if vertex_map.is_empty() {
        return 1.0;
    }

    // IEEE-754 f64 addition is not associative, so summing over a
    // `HashMap` (randomized iteration order) would let the least
    // significant bits of each component score drift across process
    // instances. Two morphisms whose true scores differ only at the
    // lsb would then swap sort order nondeterministically. Sort the
    // vertex pairs once by source name so every reduction below runs
    // in a canonical order.
    let mut vm_pairs: Vec<(&Name, &Name)> = vertex_map.iter().collect();
    vm_pairs.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));

    // 1. Name similarity component (weight 0.25)
    let name_score: f64 = {
        let mut total = 0.0;
        for (src_id, tgt_id) in &vm_pairs {
            let dist = edit_distance(src_id.as_str(), tgt_id.as_str());
            let max_len = src_id.len().max(tgt_id.len()).max(1);
            #[allow(clippy::cast_precision_loss)]
            {
                total += 1.0 - (dist as f64 / max_len as f64);
            }
        }
        #[allow(clippy::cast_precision_loss)]
        {
            total / vertex_map.len() as f64
        }
    };

    // 2. Edge name preservation component (weight 0.25)
    let edge_score: f64 = if edge_map.is_empty() {
        1.0
    } else {
        let matching = edge_map
            .iter()
            .filter(|(src_e, tgt_e)| src_e.name == tgt_e.name)
            .count();
        #[allow(clippy::cast_precision_loss)]
        {
            matching as f64 / edge_map.len() as f64
        }
    };

    // 3. Property-name Jaccard similarity (weight 0.3)
    let prop_score: f64 = {
        let mut total = 0.0;
        let mut count = 0;
        for (src_id, tgt_id) in &vm_pairs {
            let src_names: std::collections::HashSet<&str> = src
                .outgoing_edges(src_id)
                .iter()
                .filter_map(|e| e.name.as_deref())
                .collect();
            let tgt_names: std::collections::HashSet<&str> = tgt
                .outgoing_edges(tgt_id)
                .iter()
                .filter_map(|e| e.name.as_deref())
                .collect();
            if !src_names.is_empty() || !tgt_names.is_empty() {
                let intersection = src_names.intersection(&tgt_names).count();
                let union = src_names.union(&tgt_names).count();
                if union > 0 {
                    #[allow(clippy::cast_precision_loss)]
                    {
                        total += intersection as f64 / union as f64;
                    }
                    count += 1;
                }
            }
        }
        if count > 0 {
            total / f64::from(count)
        } else {
            1.0
        }
    };

    // 4. Degree similarity (weight 0.2)
    let degree_score: f64 = {
        let mut total = 0.0;
        for (src_id, tgt_id) in &vm_pairs {
            let src_deg = src.outgoing_edges(src_id).len();
            let tgt_deg = tgt.outgoing_edges(tgt_id).len();
            let max_deg = src_deg.max(tgt_deg);
            if max_deg > 0 {
                let diff = src_deg.abs_diff(tgt_deg);
                #[allow(clippy::cast_precision_loss)]
                {
                    total += 1.0 - (diff as f64 / max_deg as f64);
                }
            } else {
                total += 1.0;
            }
        }
        #[allow(clippy::cast_precision_loss)]
        {
            total / vertex_map.len() as f64
        }
    };

    #[allow(clippy::suboptimal_flops)]
    let score = weights[0] * name_score
        + weights[1] * edge_score
        + weights[2] * prop_score
        + weights[3] * degree_score;
    score
}

/// Simple edit distance (Levenshtein).
///
/// Byte level, so it counts bytes rather than characters on multi-byte input.
/// Shared with the network builder, which reads it for the name component of
/// the objective, which is why it lives beside the reference score rather than
/// inside it.
pub(crate) fn edit_distance(a: &str, b: &str) -> usize {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    let m = a_bytes.len();
    let n = b_bytes.len();

    let mut prev = (0..=n).collect::<Vec<_>>();
    let mut curr = vec![0; n + 1];

    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = usize::from(a_bytes[i - 1] != b_bytes[j - 1]);
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[n]
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;
    use panproto_schema::{Protocol, SchemaBuilder};

    const WEIGHTS: [f64; 4] = [0.25, 0.25, 0.30, 0.20];

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

    fn two_vertex_schema() -> Schema {
        let protocol = test_protocol();
        SchemaBuilder::new(&protocol)
            .vertex("root", "object", None::<&str>)
            .unwrap()
            .vertex("root.label", "string", None::<&str>)
            .unwrap()
            .edge("root", "root.label", "prop", Some("label"))
            .unwrap()
            .build()
            .unwrap()
    }

    #[test]
    fn edit_distance_is_a_metric_on_short_strings() {
        assert_eq!(edit_distance("", ""), 0);
        assert_eq!(edit_distance("abc", "abc"), 0);
        assert_eq!(edit_distance("abc", ""), 3);
        assert_eq!(edit_distance("", "abc"), 3);
        assert_eq!(edit_distance("kitten", "sitting"), 3);
        assert_eq!(edit_distance("flaw", "lawn"), 2);
    }

    #[test]
    fn an_empty_vertex_map_scores_one() {
        let schema = two_vertex_schema();
        let quality =
            reference_quality(&HashMap::new(), &HashMap::new(), &schema, &schema, WEIGHTS);
        assert_eq!(quality, 1.0);
    }

    #[test]
    fn the_identity_morphism_scores_one() {
        let schema = two_vertex_schema();
        let vertex_map: HashMap<Name, Name> = schema
            .vertices
            .keys()
            .map(|id| (id.clone(), id.clone()))
            .collect();
        let edge_map: HashMap<Edge, Edge> = schema
            .edges
            .keys()
            .map(|edge| (edge.clone(), edge.clone()))
            .collect();
        let quality = reference_quality(&vertex_map, &edge_map, &schema, &schema, WEIGHTS);
        assert!((quality - 1.0).abs() < 1e-12, "identity scored {quality}");
    }
}
