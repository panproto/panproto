//! Data lineage tracking through transforms.
//!
//! Provenance records which source fields contributed to each target field
//! and through which transform steps, enabling debugging, incremental
//! recomputation, and audit/compliance. [`compute_provenance`] serves any
//! shape by anchors; [`graph_provenance`], [`graph_fiber_at_node`] and
//! [`graph_group_by`] read a graph restrict's complement, where a dropped
//! node may feed several survivors, so fibers may overlap.

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

/// Provenance of a graph restrict, read off its source, view and
/// complement.
///
/// A kept node's source is itself. A node synthesized by expanding a source
/// edge comes from that edge's endpoints, and one synthesized by expanding a
/// contracted pair from the pair's endpoints. The map has an entry for every
/// view node, and each entry lists its source fields in ascending node id.
#[must_use]
pub fn graph_provenance(
    source: &crate::ginstance::GInstance,
    view: &crate::ginstance::GInstance,
    complement: &crate::graph_complement::GraphComplement,
) -> ProvenanceMap {
    use crate::view_ref::SynthKey;
    let field = |id: u32| {
        source.node(id).map(|node| SourceField {
            schema_path: vec![node.anchor.to_string()],
            node_id: id,
        })
    };
    let mut map = ProvenanceMap::new();
    for &id in view.nodes().keys() {
        let mut sources: Vec<u32> = match complement.synthesized_nodes.get(&id) {
            None => vec![id],
            Some(SynthKey::Expand { source_edge, .. }) => complement
                .dropped_edges
                .get(source_edge)
                .map_or_else(Vec::new, |e| vec![e.src, e.tgt]),
            Some(
                SynthKey::ContractExpand { src, tgt, .. } | SynthKey::Contract { src, tgt, .. },
            ) => {
                vec![*src, *tgt]
            }
            Some(SynthKey::ResolutionReference { holder, .. })
            | Some(SynthKey::ResolutionSpan { text: holder, .. }) => match holder {
                crate::ViewRef::Node(node) => vec![*node],
                crate::ViewRef::Row(row) => vec![row.0],
                crate::ViewRef::Arc { parent, child } => vec![*parent, *child],
                crate::ViewRef::Pair { src, tgt, .. } => vec![src.0, tgt.0],
                crate::ViewRef::Edge(_) => Vec::new(),
            },
        };
        sources.sort_unstable();
        sources.dedup();
        map.insert(
            id,
            Provenance {
                node_id: id,
                source_fields: sources.into_iter().filter_map(field).collect(),
                transform_chain: Vec::new(),
            },
        );
    }
    map
}

/// The dropped source nodes a graph restrict contracted into view node
/// `node`: those reached from it through dropped nodes, ascending.
///
/// Unlike a tree, whose dropped nodes each have one nearest surviving
/// ancestor, a graph's fibers may overlap: a dropped node that two
/// surviving nodes reach through dropped nodes lies in the fiber of each.
/// A node that is not in the view, or that nothing was contracted into, has
/// an empty fiber.
#[must_use]
pub fn graph_fiber_at_node(
    source: &crate::ginstance::GInstance,
    complement: &crate::graph_complement::GraphComplement,
    node: u32,
) -> Vec<u32> {
    if complement.dropped_nodes.contains_key(&node) || source.node(node).is_none() {
        return Vec::new();
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut frontier = vec![node];
    while let Some(at) = frontier.pop() {
        for next in source.successors(at) {
            if complement.dropped_nodes.contains_key(&next) && seen.insert(next) {
                frontier.push(next);
            }
        }
    }
    seen.into_iter().collect()
}

/// The source nodes of a graph grouped by the view anchor they land on.
///
/// A node's view anchor is the image of its anchor under the vertex map;
/// nodes whose image does not survive are in no group. Every other node
/// lies in exactly one group, and each group is ascending.
#[must_use]
pub fn graph_group_by(
    migration: &crate::wtype::CompiledMigration,
    source: &crate::ginstance::GInstance,
) -> std::collections::BTreeMap<panproto_gat::Name, Vec<u32>> {
    let mut groups: std::collections::BTreeMap<panproto_gat::Name, Vec<u32>> =
        std::collections::BTreeMap::new();
    for (&id, node) in source.nodes() {
        let image = migration
            .vertex_remap
            .get(&node.anchor)
            .unwrap_or(&node.anchor);
        if migration.surviving_verts.contains(image) {
            groups.entry(image.clone()).or_default().push(id);
        }
    }
    for ids in groups.values_mut() {
        ids.sort_unstable();
    }
    groups
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
