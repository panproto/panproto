//! Incremental contraction tracker for ancestor contraction.
//!
//! Tracks which nodes have been contracted (absorbed) into their nearest
//! surviving ancestor, allowing individual contractions to be undone
//! without recomputing the entire contraction map.

use std::collections::HashMap;

use panproto_schema::Edge;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

/// Record of a single node contraction.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContractionRecord {
    /// The original parent of the contracted node.
    pub original_parent: u32,
    /// Children that the contracted node had before contraction.
    pub children: SmallVec<u32, 4>,
    /// The original edge connecting the contracted node to its parent.
    pub original_edge: Edge,
}

/// Incremental tracker for ancestor contractions in the edit lens pipeline.
///
/// When a node is contracted, it is absorbed into the nearest surviving
/// ancestor. This tracker records those contractions and supports undoing
/// them individually.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContractionTracker {
    contracted: HashMap<u32, ContractionRecord>,
    absorptions: HashMap<u32, Vec<u32>>,
}

impl ContractionTracker {
    /// Create a new, empty contraction tracker.
    #[must_use]
    pub fn new() -> Self {
        Self {
            contracted: HashMap::new(),
            absorptions: HashMap::new(),
        }
    }

    /// Record a contraction of `node_id`.
    ///
    /// The node is absorbed into its nearest surviving ancestor, which is
    /// determined by walking up from `record.original_parent` through any
    /// already-contracted nodes.
    pub fn contract(&mut self, node_id: u32, record: ContractionRecord) {
        let surviving = self.nearest_surviving(record.original_parent);
        self.absorptions.entry(surviving).or_default().push(node_id);
        self.contracted.insert(node_id, record);
    }

    /// Undo a contraction, removing the record and cleaning up absorptions.
    ///
    /// Returns the record if the node was contracted, or `None` otherwise.
    pub fn expand(&mut self, node_id: u32) -> Option<ContractionRecord> {
        let record = self.contracted.remove(&node_id)?;

        // Remove from absorptions of the surviving ancestor
        let surviving = self.nearest_surviving(record.original_parent);
        if let Some(absorbed) = self.absorptions.get_mut(&surviving) {
            if let Some(pos) = absorbed.iter().position(|&n| n == node_id) {
                absorbed.remove(pos);
            }
            if absorbed.is_empty() {
                self.absorptions.remove(&surviving);
            }
        }

        Some(record)
    }

    /// Which contracted nodes were absorbed by the given surviving node.
    #[must_use]
    pub fn contracted_into(&self, surviving: u32) -> &[u32] {
        self.absorptions.get(&surviving).map_or(&[], Vec::as_slice)
    }

    /// Check whether a node has been contracted.
    #[must_use]
    pub fn is_contracted(&self, node_id: u32) -> bool {
        self.contracted.contains_key(&node_id)
    }

    /// Return the original parent of a contracted node.
    #[must_use]
    pub fn original_parent(&self, node_id: u32) -> Option<u32> {
        self.contracted.get(&node_id).map(|r| r.original_parent)
    }

    /// Walk up from a node through any contracted ancestors to find the
    /// nearest surviving (non-contracted) ancestor.
    fn nearest_surviving(&self, mut node: u32) -> u32 {
        while let Some(record) = self.contracted.get(&node) {
            node = record.original_parent;
        }
        node
    }
}

impl Default for ContractionTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::expect_used)]
mod tests {
    use panproto_gat::Name;
    use panproto_schema::Edge;
    use smallvec::SmallVec;

    use super::*;

    fn test_edge() -> Edge {
        Edge {
            src: Name::from("a"),
            tgt: Name::from("b"),
            kind: Name::from("prop"),
            name: None,
        }
    }

    fn make_record(parent: u32, children: &[u32]) -> ContractionRecord {
        ContractionRecord {
            original_parent: parent,
            children: children.iter().copied().collect::<SmallVec<u32, 4>>(),
            original_edge: test_edge(),
        }
    }

    #[test]
    fn contract_records_children() {
        let mut tracker = ContractionTracker::new();
        tracker.contract(5, make_record(1, &[10, 11]));

        assert!(tracker.is_contracted(5));
        let record = tracker.contracted.get(&5).unwrap();
        assert_eq!(record.children.as_slice(), &[10, 11]);
        assert_eq!(record.original_parent, 1);
    }

    #[test]
    fn expand_undoes_contraction() {
        let mut tracker = ContractionTracker::new();
        tracker.contract(5, make_record(1, &[10, 11]));

        assert!(tracker.is_contracted(5));
        let record = tracker.expand(5).unwrap();
        assert_eq!(record.original_parent, 1);

        assert!(!tracker.is_contracted(5));
        assert!(tracker.contracted_into(1).is_empty());
    }

    #[test]
    fn contracted_into_tracks_absorptions() {
        let mut tracker = ContractionTracker::new();
        // Node 5 is contracted into surviving node 1
        tracker.contract(5, make_record(1, &[10]));
        // Node 6 is also contracted into surviving node 1
        tracker.contract(6, make_record(1, &[11]));

        let absorbed = tracker.contracted_into(1);
        assert!(absorbed.contains(&5));
        assert!(absorbed.contains(&6));
        assert_eq!(absorbed.len(), 2);
    }

    #[test]
    fn is_contracted_checks_correctly() {
        let mut tracker = ContractionTracker::new();
        tracker.contract(5, make_record(1, &[10]));

        assert!(tracker.is_contracted(5));
        assert!(!tracker.is_contracted(1));
        assert!(!tracker.is_contracted(10));
        assert!(!tracker.is_contracted(999));
    }

    #[test]
    fn multiple_contractions() {
        let mut tracker = ContractionTracker::new();

        // Contract 3 into 1, 4 into 2, 5 into 2
        tracker.contract(3, make_record(1, &[30, 31]));
        tracker.contract(4, make_record(2, &[40]));
        tracker.contract(5, make_record(2, &[50, 51, 52]));

        assert!(tracker.is_contracted(3));
        assert!(tracker.is_contracted(4));
        assert!(tracker.is_contracted(5));

        assert_eq!(tracker.contracted_into(1), &[3]);
        let into_2 = tracker.contracted_into(2);
        assert!(into_2.contains(&4));
        assert!(into_2.contains(&5));

        assert_eq!(tracker.original_parent(3), Some(1));
        assert_eq!(tracker.original_parent(4), Some(2));
        assert_eq!(tracker.original_parent(5), Some(2));
        assert_eq!(tracker.original_parent(99), None);

        // Expand one
        let record = tracker.expand(4).unwrap();
        assert_eq!(record.original_parent, 2);
        assert!(!tracker.is_contracted(4));
        assert_eq!(tracker.contracted_into(2), &[5]);
    }
}
