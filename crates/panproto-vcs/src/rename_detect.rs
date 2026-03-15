//! Heuristic rename detection between schema versions.
//!
//! When a vertex disappears from an old schema and a structurally
//! similar vertex appears in a new schema, there is a good chance
//! the vertex was *renamed* rather than deleted-and-added. This
//! module provides functions to detect such renames with a confidence
//! score.
//!
//! The algorithm computes pairwise structural similarity between
//! removed and added vertices, then performs greedy bipartite matching
//! to find the most likely rename pairs above a configurable threshold.

use panproto_gat::{Name, NameSite, SiteRename};
use panproto_schema::Schema;

/// A detected rename with a confidence score.
#[derive(Clone, Debug)]
pub struct DetectedRename {
    /// The site-qualified rename operation.
    pub rename: SiteRename,
    /// Confidence in \[0.0, 1.0\], based on structural similarity.
    pub confidence: f64,
}

/// Detect likely vertex renames between two schema versions.
///
/// Compares removed vertices against added vertices using structural
/// similarity:
///   - Same vertex kind: +0.3
///   - Same set of outgoing edge names: +0.3
///   - Same set of incoming edge names: +0.2
///   - Edit distance of vertex ID names ≤ 3: +0.2
///
/// Returns pairs scoring ≥ `threshold` after greedy bipartite matching
/// (highest score first).
#[must_use]
pub fn detect_vertex_renames(old: &Schema, new: &Schema, threshold: f64) -> Vec<DetectedRename> {
    // Collect removed and added vertex IDs
    let removed: Vec<&Name> = old
        .vertices
        .keys()
        .filter(|id| !new.vertices.contains_key(id.as_str()))
        .collect();

    let added: Vec<&Name> = new
        .vertices
        .keys()
        .filter(|id| !old.vertices.contains_key(id.as_str()))
        .collect();

    if removed.is_empty() || added.is_empty() {
        return Vec::new();
    }

    // Compute pairwise similarity scores
    let mut candidates: Vec<(usize, usize, f64)> = Vec::new();

    for (ri, &rem_id) in removed.iter().enumerate() {
        let rem_vertex = &old.vertices[rem_id];

        for (ai, &add_id) in added.iter().enumerate() {
            let add_vertex = &new.vertices[add_id];
            let mut score = 0.0;

            // Same vertex kind: +0.3
            if rem_vertex.kind == add_vertex.kind {
                score += 0.3;
            }

            // Same outgoing edge names: +0.3
            let rem_out: std::collections::HashSet<Option<&str>> = old
                .outgoing_edges(rem_id)
                .iter()
                .map(|e| e.name.as_deref())
                .collect();
            let add_out: std::collections::HashSet<Option<&str>> = new
                .outgoing_edges(add_id)
                .iter()
                .map(|e| e.name.as_deref())
                .collect();
            if !rem_out.is_empty() && rem_out == add_out {
                score += 0.3;
            }

            // Same incoming edge names: +0.2
            let rem_in: std::collections::HashSet<Option<&str>> = old
                .incoming_edges(rem_id)
                .iter()
                .map(|e| e.name.as_deref())
                .collect();
            let add_in: std::collections::HashSet<Option<&str>> = new
                .incoming_edges(add_id)
                .iter()
                .map(|e| e.name.as_deref())
                .collect();
            if !rem_in.is_empty() && rem_in == add_in {
                score += 0.2;
            }

            // Edit distance of names ≤ 3: +0.2
            if edit_distance(rem_id.as_str(), add_id.as_str()) <= 3 {
                score += 0.2;
            }

            if score >= threshold {
                candidates.push((ri, ai, score));
            }
        }
    }

    // Greedy bipartite matching: take highest-scoring pairs first
    candidates.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

    let mut used_removed = vec![false; removed.len()];
    let mut used_added = vec![false; added.len()];
    let mut result = Vec::new();

    for (ri, ai, score) in candidates {
        if used_removed[ri] || used_added[ai] {
            continue;
        }
        used_removed[ri] = true;
        used_added[ai] = true;

        result.push(DetectedRename {
            rename: SiteRename::new(NameSite::VertexId, removed[ri].as_str(), added[ai].as_str()),
            confidence: score,
        });
    }

    result
}

/// Detect likely edge label renames between two schema versions.
///
/// For edges between the same surviving vertex pair, compares removed
/// edge labels against added edge labels using name edit distance.
#[must_use]
pub fn detect_edge_renames(old: &Schema, new: &Schema, threshold: f64) -> Vec<DetectedRename> {
    let mut result = Vec::new();

    // Find vertex pairs that exist in both schemas
    for old_edge in old.edges.keys() {
        if new.vertices.contains_key(old_edge.src.as_str())
            && new.vertices.contains_key(old_edge.tgt.as_str())
        {
            let Some(old_name) = old_edge.name.as_deref() else {
                continue;
            };

            // Check if this exact edge exists in new (then no rename)
            if new.edges.contains_key(old_edge) {
                continue;
            }

            // Find new edges between the same vertex pair with different names
            for new_edge in new.edges.keys() {
                if new_edge.src == old_edge.src
                    && new_edge.tgt == old_edge.tgt
                    && new_edge.kind == old_edge.kind
                {
                    let Some(new_name) = new_edge.name.as_deref() else {
                        continue;
                    };

                    if new_name == old_name {
                        continue;
                    }

                    // Score based on edit distance
                    let dist = edit_distance(old_name, new_name);
                    let max_len = old_name.len().max(new_name.len());
                    let score = if max_len == 0 {
                        0.0
                    } else {
                        #[allow(clippy::cast_precision_loss)]
                        {
                            1.0 - (dist as f64 / max_len as f64)
                        }
                    };

                    if score >= threshold {
                        result.push(DetectedRename {
                            rename: SiteRename::new(NameSite::EdgeLabel, old_name, new_name),
                            confidence: score,
                        });
                    }
                }
            }
        }
    }

    result
}

/// Simple edit distance (Levenshtein) between two strings.
fn edit_distance(a: &str, b: &str) -> usize {
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
            obj_kinds: vec!["object".into(), "string".into(), "integer".into()],
            constraint_sorts: vec![],
            ..Protocol::default()
        }
    }

    fn build_schema(vertices: &[(&str, &str)], edges: &[(&str, &str, &str, &str)]) -> Schema {
        let proto = test_protocol();
        let mut builder = SchemaBuilder::new(&proto);
        for (id, kind) in vertices {
            builder = builder.vertex(id, kind, None::<&str>).unwrap();
        }
        for (src, tgt, kind, name) in edges {
            builder = builder.edge(src, tgt, kind, Some(*name)).unwrap();
        }
        builder.build().unwrap()
    }

    #[test]
    fn detect_vertex_rename_same_kind_same_edges() {
        let old = build_schema(
            &[("root", "object"), ("root.text", "string")],
            &[("root", "root.text", "prop", "text")],
        );
        let new = build_schema(
            &[("root", "object"), ("root.body", "string")],
            &[("root", "root.body", "prop", "body")],
        );

        // root.text → root.body: same kind (string), different name
        let renames = detect_vertex_renames(&old, &new, 0.3);
        assert!(!renames.is_empty(), "should detect a rename");
        assert_eq!(renames[0].rename.old.as_ref(), "root.text");
        assert_eq!(renames[0].rename.new.as_ref(), "root.body");
        assert!(renames[0].confidence >= 0.3);
    }

    #[test]
    fn no_rename_when_different_kind() {
        let old = build_schema(
            &[("root", "object"), ("root.count", "integer")],
            &[("root", "root.count", "prop", "count")],
        );
        let new = build_schema(
            &[("root", "object"), ("root.label", "string")],
            &[("root", "root.label", "prop", "label")],
        );

        let renames = detect_vertex_renames(&old, &new, 0.5);
        assert!(
            renames.is_empty(),
            "different kinds should not match at 0.5 threshold"
        );
    }

    #[test]
    fn no_rename_when_nothing_changed() {
        let schema = build_schema(
            &[("root", "object"), ("root.text", "string")],
            &[("root", "root.text", "prop", "text")],
        );

        let renames = detect_vertex_renames(&schema, &schema, 0.3);
        assert!(
            renames.is_empty(),
            "identical schemas should have no renames"
        );
    }

    #[test]
    fn edit_distance_basic() {
        assert_eq!(edit_distance("text", "text"), 0);
        assert_eq!(edit_distance("text", "body"), 4);
        assert_eq!(edit_distance("text", "texts"), 1);
        assert_eq!(edit_distance("", "abc"), 3);
        assert_eq!(edit_distance("abc", ""), 3);
    }

    #[test]
    fn detect_edge_rename() {
        let old = build_schema(
            &[("root", "object"), ("root.x", "string")],
            &[("root", "root.x", "prop", "label")],
        );

        // Same structure, but the edge label changed from "label" to "labels"
        // (similar enough to exceed the threshold)
        let proto = test_protocol();
        let new = SchemaBuilder::new(&proto)
            .vertex("root", "object", None::<&str>)
            .unwrap()
            .vertex("root.x", "string", None::<&str>)
            .unwrap()
            .edge("root", "root.x", "prop", Some("labels"))
            .unwrap()
            .build()
            .unwrap();

        let renames = detect_edge_renames(&old, &new, 0.3);
        assert!(!renames.is_empty(), "should detect edge rename");
        assert_eq!(renames[0].rename.old.as_ref(), "label");
        assert_eq!(renames[0].rename.new.as_ref(), "labels");
    }
}
