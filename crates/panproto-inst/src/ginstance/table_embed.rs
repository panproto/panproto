//! Tables as graph instances.
//!
//! [`GInstance::from_finstance`] lays a table instance out as a graph whose
//! nodes are rows and whose edges are foreign-key references.

use std::collections::HashMap;

use panproto_gat::Name;

use super::{EdgeId, GEdge, GInstance};
use crate::error::InstError;
use crate::functor::FInstance;
use crate::metadata::Node;

impl GInstance {
    /// The graph form of a table instance.
    ///
    /// Tables are taken in ascending name order and each table's rows in
    /// stored order; the `n`th row so reached becomes node `n`, anchored at
    /// its table name, with the row's columns as its extra fields and no
    /// value. Foreign keys are taken in ascending schema-edge order and each
    /// key's pairs in stored order; the `m`th pair so reached becomes edge
    /// `EdgeId(m)` from the source row's node to the target row's node,
    /// labelled with the foreign-key edge and carrying no position.
    ///
    /// # Errors
    ///
    /// Returns [`InstError::DanglingForeignKey`] when a pair names a row, or a
    /// table, that does not exist, and [`InstError::NodeIdsExhausted`] or
    /// [`InstError::EdgeIdsExhausted`] when the counts do not fit in `u32`.
    pub fn from_finstance(tables: &FInstance) -> Result<Self, InstError> {
        let mut graph = Self::new();
        let mut names: Vec<&String> = tables.tables.keys().collect();
        names.sort_unstable();
        let mut first_id: HashMap<&str, u32> = HashMap::new();
        let mut next: u32 = 0;
        for name in names {
            first_id.insert(name.as_str(), next);
            for row in &tables.tables[name] {
                let mut node = Node::new(next, Name::from(name.as_str()));
                node.extra_fields.clone_from(row);
                graph.insert_node(node)?;
                next = next.checked_add(1).ok_or(InstError::NodeIdsExhausted)?;
            }
        }

        let row_node = |table: &str, row: usize, edge_kind: &Name| {
            let dangling = || InstError::DanglingForeignKey {
                edge_kind: edge_kind.to_string(),
                table: table.to_owned(),
                row,
            };
            let rows = tables.tables.get(table).ok_or_else(dangling)?;
            if row >= rows.len() {
                return Err(dangling());
            }
            let offset = u32::try_from(row).map_err(|_| InstError::NodeIdsExhausted)?;
            first_id
                .get(table)
                .and_then(|base| base.checked_add(offset))
                .ok_or_else(dangling)
        };

        let mut keys: Vec<_> = tables.foreign_keys.keys().collect();
        keys.sort_unstable();
        let mut next_edge: u32 = 0;
        for key in keys {
            for &(src_row, tgt_row) in &tables.foreign_keys[key] {
                let src = row_node(&key.src, src_row, &key.kind)?;
                let tgt = row_node(&key.tgt, tgt_row, &key.kind)?;
                graph.insert_edge(EdgeId(next_edge), GEdge::new(src, tgt, key.clone()))?;
                next_edge = next_edge
                    .checked_add(1)
                    .ok_or(InstError::EdgeIdsExhausted)?;
            }
        }
        Ok(graph)
    }
}
