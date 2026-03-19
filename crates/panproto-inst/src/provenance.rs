//! Data lineage tracking through transforms.
//!
//! Provenance records which source fields contributed to each target field
//! and through which transform steps, enabling debugging, incremental
//! recomputation, and audit/compliance.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Provenance information for a single node in the target instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provenance {
    /// The node ID in the target instance.
    pub node_id: u32,
    /// Source fields that contributed to this node's value.
    pub source_fields: Vec<SourceField>,
    /// Transform steps that were applied.
    pub transform_chain: Vec<TransformStep>,
}

/// A reference to a source field that contributed to a target value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceField {
    /// Path of schema vertex names from root to the source field.
    pub schema_path: Vec<String>,
    /// Node ID in the source instance.
    pub node_id: u32,
}

/// A step in the transform chain that produced a value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransformStep {
    /// Name of the protolens that performed this step.
    pub protolens_name: String,
    /// Index of this step in the protolens chain.
    pub step_index: usize,
}

/// A map from target node IDs to their provenance information.
pub type ProvenanceMap = HashMap<u32, Provenance>;

/// Compute provenance for a restriction operation.
///
/// Given source and target node lists and a vertex remapping,
/// build a provenance map recording which source nodes contributed
/// to each target node.
#[must_use]
pub fn compute_provenance(
    src_nodes: &[(u32, String)],
    tgt_nodes: &[(u32, String)],
    vertex_remap: &HashMap<String, String>,
) -> ProvenanceMap {
    let mut map = ProvenanceMap::new();
    for (tgt_id, tgt_anchor) in tgt_nodes {
        let source_fields: Vec<SourceField> = src_nodes
            .iter()
            .filter(|(_, src_anchor)| {
                vertex_remap
                    .get(src_anchor.as_str())
                    .is_some_and(|mapped| mapped == tgt_anchor)
                    || src_anchor == tgt_anchor
            })
            .map(|(src_id, src_anchor)| SourceField {
                schema_path: vec![src_anchor.clone()],
                node_id: *src_id,
            })
            .collect();

        map.insert(
            *tgt_id,
            Provenance {
                node_id: *tgt_id,
                source_fields,
                transform_chain: vec![],
            },
        );
    }
    map
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn identity_provenance_maps_nodes_to_themselves() {
        let src = vec![
            (0, "root".to_owned()),
            (1, "field_a".to_owned()),
            (2, "field_b".to_owned()),
        ];
        let tgt = vec![
            (0, "root".to_owned()),
            (1, "field_a".to_owned()),
            (2, "field_b".to_owned()),
        ];
        let remap = HashMap::new();
        let prov = compute_provenance(&src, &tgt, &remap);

        assert_eq!(prov.len(), 3);
        // Each target node should have exactly one source field (itself).
        for (tgt_id, p) in &prov {
            assert_eq!(p.source_fields.len(), 1, "node {tgt_id} source count");
            assert_eq!(p.source_fields[0].node_id, *tgt_id);
        }
    }

    #[test]
    fn renamed_vertex_provenance_follows_remap() {
        let src = vec![(1, "old_name".to_owned())];
        let tgt = vec![(1, "new_name".to_owned())];
        let mut remap = HashMap::new();
        remap.insert("old_name".to_owned(), "new_name".to_owned());

        let prov = compute_provenance(&src, &tgt, &remap);
        assert_eq!(prov.len(), 1);
        let p = &prov[&1];
        assert_eq!(p.source_fields.len(), 1);
        assert_eq!(p.source_fields[0].schema_path, vec!["old_name".to_owned()]);
    }

    #[test]
    fn no_matching_source_yields_empty_sources() {
        let src = vec![(1, "unrelated".to_owned())];
        let tgt = vec![(2, "target_only".to_owned())];
        let remap = HashMap::new();

        let prov = compute_provenance(&src, &tgt, &remap);
        assert_eq!(prov.len(), 1);
        assert!(prov[&2].source_fields.is_empty());
    }
}
