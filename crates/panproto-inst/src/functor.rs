//! Set-valued functor instance representation.
//!
//! An [`FInstance`] represents relational (tabular) data as a set-valued
//! functor: each schema vertex maps to a table (set of rows), and each
//! edge maps to a foreign-key relationship.
//!
//! The restrict operation (`functor_restrict`) is precomposition
//! (`Delta_F`): for each table in the target, look up the corresponding
//! source table.

use std::collections::HashMap;

use panproto_schema::Edge;
use serde::{Deserialize, Serialize};

use crate::error::RestrictError;
use crate::value::Value;
use crate::wtype::CompiledMigration;

/// A set-valued functor instance (relational data).
///
/// Tables map schema vertex IDs to rows (each row is a map of column
/// names to values). Foreign keys map schema edges to pairs of
/// (source row index, target row index).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FInstance {
    /// Tables: vertex ID to rows. Each row is a column-name to value map.
    pub tables: HashMap<String, Vec<HashMap<String, Value>>>,
    /// Foreign keys: edge to row-index pairs.
    pub foreign_keys: HashMap<Edge, Vec<(usize, usize)>>,
}

impl FInstance {
    /// Create a new empty functor instance.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tables: HashMap::new(),
            foreign_keys: HashMap::new(),
        }
    }

    /// Add a table for the given vertex.
    #[must_use]
    pub fn with_table(
        mut self,
        vertex_id: impl Into<String>,
        rows: Vec<HashMap<String, Value>>,
    ) -> Self {
        self.tables.insert(vertex_id.into(), rows);
        self
    }

    /// Add a foreign key for the given edge.
    #[must_use]
    pub fn with_foreign_key(mut self, edge: Edge, pairs: Vec<(usize, usize)>) -> Self {
        self.foreign_keys.insert(edge, pairs);
        self
    }

    /// Returns the number of tables.
    #[must_use]
    pub fn table_count(&self) -> usize {
        self.tables.len()
    }

    /// Returns the number of rows in a specific table.
    #[must_use]
    pub fn row_count(&self, vertex_id: &str) -> usize {
        self.tables.get(vertex_id).map_or(0, Vec::len)
    }
}

impl Default for FInstance {
    fn default() -> Self {
        Self::new()
    }
}

/// The restrict operation for set-valued functor instances.
///
/// This is `Delta_F` (precomposition): for each vertex in the target
/// schema, look up the corresponding table in the source via the
/// migration's vertex map.
///
/// # Errors
///
/// Returns `RestrictError` if a required source table is missing
/// (though this typically means the migration is malformed).
pub fn functor_restrict(
    instance: &FInstance,
    migration: &CompiledMigration,
) -> Result<FInstance, RestrictError> {
    let mut new_tables = HashMap::new();
    let mut new_fks = HashMap::new();

    // For each surviving vertex, pull the table from the source.
    // vertex_remap maps src -> tgt, so invert to find all sources.
    // When multiple source vertices map to the same target, collect all.
    for tgt_vertex in &migration.surviving_verts {
        let src_vertices: Vec<&str> = migration
            .vertex_remap
            .iter()
            .filter(|(_, v)| *v == tgt_vertex)
            .map(|(k, _)| &**k)
            .collect();

        let sources = if src_vertices.is_empty() {
            vec![&**tgt_vertex]
        } else {
            src_vertices
        };

        let mut combined_rows = Vec::new();
        for src_vertex in &sources {
            if let Some(rows) = instance.tables.get(*src_vertex) {
                combined_rows.extend(rows.iter().cloned());
            }
        }
        if !combined_rows.is_empty() {
            new_tables.insert(tgt_vertex.to_string(), combined_rows);
        }
    }

    // Remap foreign keys for surviving edges
    for (edge, pairs) in &instance.foreign_keys {
        if let Some(new_edge) = migration.edge_remap.get(edge) {
            if migration.surviving_verts.contains(&new_edge.src)
                && migration.surviving_verts.contains(&new_edge.tgt)
            {
                new_fks.insert(new_edge.clone(), pairs.clone());
            }
        } else if migration.surviving_edges.contains(edge) {
            new_fks.insert(edge.clone(), pairs.clone());
        }
    }

    Ok(FInstance {
        tables: new_tables,
        foreign_keys: new_fks,
    })
}

/// The extend operation for set-valued functor instances (`Sigma_F`).
///
/// This is the left Kan extension: given an instance of the source schema
/// and a migration mapping (source -> target), produce an instance of the
/// target schema by copying tables forward and initializing unmapped tables
/// as empty.
///
/// # Errors
///
/// Returns `RestrictError` if the migration references inconsistent mappings.
pub fn functor_extend(
    instance: &FInstance,
    migration: &CompiledMigration,
) -> Result<FInstance, RestrictError> {
    let mut new_tables = HashMap::new();
    let mut new_fks = HashMap::new();

    // Copy tables from source to their mapped names in the target.
    // vertex_remap maps src -> tgt. When multiple source vertices map
    // to the same target (many-to-one), compute the coproduct: disjoint
    // union of rows with original column names (they share the same
    // schema vertex, so columns should match). Row indices in FK pairs
    // are offset by the cumulative row count to remain valid after
    // concatenation. Missing columns across source tables are filled
    // with Value::Null.

    // First pass: collect rows per target vertex and track row offsets
    // per source vertex for FK index offsetting.
    let mut row_offsets: HashMap<String, usize> = HashMap::with_capacity(instance.tables.len());
    for (src_vertex, rows) in &instance.tables {
        let tgt_vertex = migration
            .vertex_remap
            .get(src_vertex.as_str())
            .map_or_else(|| src_vertex.clone(), std::string::ToString::to_string);
        let entry = new_tables.entry(tgt_vertex).or_insert_with(Vec::new);
        let offset = entry.len();
        row_offsets.insert(src_vertex.clone(), offset);
        entry.extend(rows.iter().cloned());
    }

    // Second pass: union column sets within each target table and fill
    // missing values with Value::Null.
    for rows in new_tables.values_mut() {
        // Collect the union of all column names across rows.
        let all_columns: std::collections::HashSet<String> =
            rows.iter().flat_map(|row| row.keys().cloned()).collect();
        // Fill missing columns with null.
        for row in rows.iter_mut() {
            for col in &all_columns {
                row.entry(col.clone()).or_insert(Value::Null);
            }
        }
    }

    // Initialize tables that exist in surviving_verts but were not
    // populated by the source instance.
    for tgt_vertex in &migration.surviving_verts {
        new_tables
            .entry(tgt_vertex.to_string())
            .or_insert_with(Vec::new);
    }

    // Remap foreign keys, offsetting row indices by the cumulative row
    // count so they remain valid after concatenation.
    for (edge, pairs) in &instance.foreign_keys {
        let resolved_edge = migration.edge_remap.get(edge).map_or_else(
            || {
                if migration.surviving_edges.contains(edge) {
                    Some(edge.clone())
                } else {
                    None
                }
            },
            |new_edge| Some(new_edge.clone()),
        );

        if let Some(new_edge) = resolved_edge {
            let src_offset = row_offsets.get(&*edge.src).copied().unwrap_or(0);
            let tgt_offset = row_offsets.get(&*edge.tgt).copied().unwrap_or(0);
            let offset_pairs: Vec<(usize, usize)> = pairs
                .iter()
                .map(|(s, t)| (s + src_offset, t + tgt_offset))
                .collect();
            new_fks.insert(new_edge, offset_pairs);
        }
    }

    Ok(FInstance {
        tables: new_tables,
        foreign_keys: new_fks,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn empty_functor_instance() {
        let inst = FInstance::new();
        assert_eq!(inst.table_count(), 0);
    }

    #[test]
    fn functor_with_tables() {
        let mut row = HashMap::new();
        row.insert("name".to_string(), Value::Str("Alice".into()));

        let inst = FInstance::new().with_table("users", vec![row]);
        assert_eq!(inst.table_count(), 1);
        assert_eq!(inst.row_count("users"), 1);
        assert_eq!(inst.row_count("posts"), 0);
    }

    #[test]
    fn functor_restrict_drops_table() {
        let mut users_row = HashMap::new();
        users_row.insert("name".to_string(), Value::Str("Alice".into()));

        let mut posts_row = HashMap::new();
        posts_row.insert("title".to_string(), Value::Str("Hello".into()));

        let fk_edge = Edge {
            src: "posts".into(),
            tgt: "users".into(),
            kind: "fk".into(),
            name: Some("author".into()),
        };

        let inst = FInstance::new()
            .with_table("users", vec![users_row])
            .with_table("posts", vec![posts_row])
            .with_foreign_key(fk_edge, vec![(0, 0)]);

        // Migration that only keeps "users"
        let migration = CompiledMigration {
            surviving_verts: HashSet::from([panproto_gat::Name::from("users")]),
            surviving_edges: HashSet::new(),
            vertex_remap: HashMap::new(),
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
        };

        let result = functor_restrict(&inst, &migration);
        assert!(result.is_ok());
        let restricted = result.unwrap_or_else(|_| FInstance::new());
        assert_eq!(restricted.table_count(), 1);
        assert!(restricted.tables.contains_key("users"));
        assert!(!restricted.tables.contains_key("posts"));
        assert!(restricted.foreign_keys.is_empty());
    }
}
