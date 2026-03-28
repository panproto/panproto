//! Hyperedge fan representation.
//!
//! A [`Fan`] represents the instance-level realization of a schema
//! hyper-edge: a parent node connected to multiple child nodes via
//! labeled positions from the hyper-edge's signature.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// A hyperedge fan in a W-type instance.
///
/// Fans capture multi-way relationships. For example, a SQL foreign key
/// is a 4-ary hyper-edge connecting a table, a column, a referenced
/// table, and a referenced column.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fan {
    /// The schema hyper-edge this fan instantiates.
    pub hyper_edge_id: String,
    /// The parent node ID.
    pub parent: u32,
    /// Labeled child positions: label name to node ID.
    pub children: HashMap<String, u32>,
}

impl Fan {
    /// Create a new fan for the given hyper-edge.
    #[must_use]
    pub fn new(hyper_edge_id: impl Into<String>, parent: u32) -> Self {
        Self {
            hyper_edge_id: hyper_edge_id.into(),
            parent,
            children: HashMap::new(),
        }
    }

    /// Add a labeled child to the fan.
    #[must_use]
    pub fn with_child(mut self, label: impl Into<String>, node_id: u32) -> Self {
        self.children.insert(label.into(), node_id);
        self
    }

    /// Returns the arity (number of children) of this fan.
    #[must_use]
    pub fn arity(&self) -> usize {
        self.children.len()
    }

    /// Returns the node ID for the given label, if present.
    #[must_use]
    pub fn child(&self, label: &str) -> Option<u32> {
        self.children.get(label).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fan_construction() {
        let fan = Fan::new("fk_posts_users", 0)
            .with_child("table", 1)
            .with_child("column", 2)
            .with_child("ref_table", 3)
            .with_child("ref_column", 4);

        assert_eq!(fan.arity(), 4);
        assert_eq!(fan.child("table"), Some(1));
        assert_eq!(fan.child("column"), Some(2));
        assert_eq!(fan.child("missing"), None);
    }
}
