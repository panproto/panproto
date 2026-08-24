//! Set-valued functor instance representation.
//!
//! An [`FInstance`] represents relational (tabular) data as a set-valued
//! functor: each schema vertex maps to a table (set of rows), and each
//! edge maps to a foreign-key relationship.
//!
//! Both migration operations here carry an `S`-instance forward along a
//! migration `F: S -> T`, and they differ in what they do with the structure
//! `F` does not name. [`functor_restrict`] keeps the surviving fragment: a
//! vertex's rows land in its image's table, a vertex the migration drops
//! contributes none, and only the edges it remaps or lets survive carry their
//! foreign keys across. [`functor_extend`] keeps everything: a vertex the
//! migration says nothing about travels under its own name, merged rows are
//! padded to a common column set, and every surviving vertex gets a table even
//! when no rows reach it.
//!
//! Neither is precomposition. Both take the coproduct over a fibre when the
//! vertex map merges two vertices, which is what `Sigma_F` does. `Delta_F`
//! runs the other way, from a `T`-instance to an `S`-instance, and lives in
//! [`crate::adjunction::f_delta`] with the rest of the adjunction.

use std::collections::HashMap;

use panproto_gat::Name;
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

/// Carry a set-valued functor instance forward along a migration, keeping
/// the fragment that survives.
///
/// Each surviving target vertex takes the rows of every source vertex the
/// migration sends to it, concatenated in source-name order and rewritten by
/// that vertex's op-to-term assignments; a target no source reaches keeps a
/// same-named source table when the instance has one. Foreign keys travel for
/// the edges the migration remaps between surviving vertices and for those it
/// lets survive, with row indices offset into the block each source table
/// occupies in the concatenation.
///
/// This is a pushforward, not precomposition: over a vertex map that merges
/// two vertices it returns the fibre's coproduct, which is `Sigma_F`'s answer.
/// For `Delta_F` see [`crate::adjunction::f_delta`].
///
/// # Errors
///
/// Returns `RestrictError` if a row's op-to-term assignment fails to
/// evaluate.
pub fn functor_restrict(
    instance: &FInstance,
    migration: &CompiledMigration,
) -> Result<FInstance, RestrictError> {
    let mut new_tables = HashMap::new();
    let mut new_fks: HashMap<Edge, Vec<(usize, usize)>> = HashMap::new();
    // Position of each source vertex's row block inside the concatenated
    // target table, so foreign-key row indices survive the merge.
    let mut row_offsets: HashMap<&str, usize> = HashMap::new();

    // For each surviving vertex, pull the table from the source.
    // vertex_remap maps src -> tgt, so invert to find all sources.
    // When multiple source vertices map to the same target, their tables are
    // concatenated in source-name order so the result does not depend on hash
    // iteration order.
    let mut targets: Vec<&Name> = migration.surviving_verts.iter().collect();
    targets.sort_unstable();
    for tgt_vertex in targets {
        let mut src_vertices: Vec<&str> = migration
            .vertex_remap
            .iter()
            .filter(|(_, v)| *v == tgt_vertex)
            .map(|(k, _)| &**k)
            .collect();
        src_vertices.sort_unstable();

        let sources = if src_vertices.is_empty() {
            vec![&**tgt_vertex]
        } else {
            src_vertices
        };

        let mut combined_rows = Vec::new();
        for src_vertex in sources {
            row_offsets.insert(src_vertex, combined_rows.len());
            if let Some(rows) = instance.tables.get(src_vertex) {
                // The migration acts on values by substitution: each carried
                // row is rewritten by the source vertex's op-to-term
                // assignments.
                let assignments = migration.op_term_assignments.get(src_vertex);
                for row in rows {
                    let mut new_row = row.clone();
                    if let Some(assignments) = assignments {
                        crate::wtype::apply_term_assignments_to_row(&mut new_row, assignments)?;
                    }
                    combined_rows.push(new_row);
                }
            }
        }
        if !combined_rows.is_empty() {
            new_tables.insert(tgt_vertex.to_string(), combined_rows);
        }
    }

    // Remap foreign keys for surviving edges, offsetting each pair into the
    // block its source and target rows occupy after concatenation. Source
    // edges are visited in sorted order and edges colliding under
    // `edge_remap` have their pair sets unioned rather than overwritten.
    let mut src_edges: Vec<&Edge> = instance.foreign_keys.keys().collect();
    src_edges.sort_unstable();
    for edge in src_edges {
        let Some(pairs) = instance.foreign_keys.get(edge) else {
            continue;
        };
        let new_edge = if let Some(new_edge) = migration.edge_remap.get(edge) {
            if !migration.surviving_verts.contains(&new_edge.src)
                || !migration.surviving_verts.contains(&new_edge.tgt)
            {
                continue;
            }
            new_edge.clone()
        } else if migration.surviving_edges.contains(edge) {
            edge.clone()
        } else {
            continue;
        };

        let src_offset = row_offsets.get(&*edge.src).copied().unwrap_or(0);
        let tgt_offset = row_offsets.get(&*edge.tgt).copied().unwrap_or(0);
        let entry = new_fks.entry(new_edge).or_default();
        for (s, t) in pairs {
            let remapped = (s + src_offset, t + tgt_offset);
            if !entry.contains(&remapped) {
                entry.push(remapped);
            }
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
    let mut new_fks: HashMap<Edge, Vec<(usize, usize)>> = HashMap::new();

    // Copy tables from source to their mapped names in the target.
    // vertex_remap maps src -> tgt. When multiple source vertices map
    // to the same target (many-to-one), compute the coproduct: disjoint
    // union of rows with original column names (they share the same
    // schema vertex, so columns should match). Row indices in FK pairs
    // are offset by the cumulative row count to remain valid after
    // concatenation. Missing columns across source tables are filled
    // with Value::Null.

    // First pass: collect rows per target vertex and track row offsets
    // per source vertex for FK index offsetting. Source vertices are visited
    // in name order so the concatenation, and the offsets derived from it, do
    // not depend on hash iteration order.
    let mut row_offsets: HashMap<String, usize> = HashMap::with_capacity(instance.tables.len());
    let mut src_vertices: Vec<&String> = instance.tables.keys().collect();
    src_vertices.sort_unstable();
    for src_vertex in src_vertices {
        let Some(rows) = instance.tables.get(src_vertex) else {
            continue;
        };
        let tgt_vertex = migration
            .vertex_remap
            .get(src_vertex.as_str())
            .map_or_else(|| src_vertex.clone(), std::string::ToString::to_string);
        let entry = new_tables.entry(tgt_vertex).or_insert_with(Vec::new);
        let offset = entry.len();
        row_offsets.insert(src_vertex.clone(), offset);
        // Sigma acts on values by substitution: each forwarded row is
        // rewritten by the source vertex's op-to-term assignments before
        // the tables are concatenated.
        let assignments = migration.op_term_assignments.get(src_vertex.as_str());
        for row in rows {
            let mut new_row = row.clone();
            if let Some(assignments) = assignments {
                crate::wtype::apply_term_assignments_to_row(&mut new_row, assignments)?;
            }
            entry.push(new_row);
        }
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
    let mut src_edges: Vec<&Edge> = instance.foreign_keys.keys().collect();
    src_edges.sort_unstable();
    for edge in src_edges {
        let Some(pairs) = instance.foreign_keys.get(edge) else {
            continue;
        };
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
            let entry: &mut Vec<(usize, usize)> = new_fks.entry(new_edge).or_default();
            for (s, t) in pairs {
                let remapped = (s + src_offset, t + tgt_offset);
                if !entry.contains(&remapped) {
                    entry.push(remapped);
                }
            }
        }
    }

    Ok(FInstance {
        tables: new_tables,
        foreign_keys: new_fks,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::wtype::{TermAssignment, TermScope};

    /// A `full = first ++ " " ++ last` computed-column term.
    fn full_name_term() -> panproto_expr::Expr {
        use panproto_expr::{BuiltinOp, Expr, Literal};
        use std::sync::Arc;
        Expr::Builtin(
            BuiltinOp::Concat,
            vec![
                Expr::Var(Arc::from("first")),
                Expr::Builtin(
                    BuiltinOp::Concat,
                    vec![
                        Expr::Lit(Literal::Str(" ".into())),
                        Expr::Var(Arc::from("last")),
                    ],
                ),
            ],
        )
    }

    fn person_row() -> HashMap<String, Value> {
        let mut row = HashMap::new();
        row.insert("first".to_string(), Value::Str("Ada".into()));
        row.insert("last".to_string(), Value::Str("Lovelace".into()));
        row
    }

    fn computed_full_migration() -> CompiledMigration {
        let mut migration = CompiledMigration::default();
        migration.surviving_verts.insert("person".into());
        migration.op_term_assignments.insert(
            "person".into(),
            vec![TermAssignment::Compute {
                target: "full".into(),
                scope: TermScope::Row,
                term: full_name_term(),
                inverse: None,
                coercion_class: panproto_gat::CoercionClass::Projection,
            }],
        );
        migration
    }

    #[test]
    fn sigma_acts_by_substitution() {
        // Sigma (left Kan extension) computes the migrated `full` column by
        // substituting each source row's `first`/`last` values into the term.
        let inst = FInstance::new().with_table("person", vec![person_row()]);
        let extended = functor_extend(&inst, &computed_full_migration())
            .expect("functor_extend should succeed");
        let rows = extended.tables.get("person").expect("person table present");
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].get("full"),
            Some(&Value::Str("Ada Lovelace".into())),
            "Sigma must compute the derived column by substitution",
        );
        // Source columns pass through unchanged.
        assert_eq!(rows[0].get("first"), Some(&Value::Str("Ada".into())));
    }

    #[test]
    fn computed_column_migration_e2e() {
        // The surviving-fragment form acts on values by substitution too: the
        // migrated `person` rows gain the computed `full` column end-to-end.
        let inst = FInstance::new().with_table("person", vec![person_row()]);
        let restricted = functor_restrict(&inst, &computed_full_migration())
            .expect("functor_restrict should succeed");
        let rows = restricted
            .tables
            .get("person")
            .expect("person table present");
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].get("full"),
            Some(&Value::Str("Ada Lovelace".into()))
        );
        assert_eq!(rows[0].get("last"), Some(&Value::Str("Lovelace".into())));
    }

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
            conditional_survival: HashMap::new(),
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        };

        let result = functor_restrict(&inst, &migration);
        assert!(result.is_ok());
        let restricted = result.unwrap_or_else(|_| FInstance::new());
        assert_eq!(restricted.table_count(), 1);
        assert!(restricted.tables.contains_key("users"));
        assert!(!restricted.tables.contains_key("posts"));
        assert!(restricted.foreign_keys.is_empty());
    }
    /// Two source tables merged into one target: FK row indices must be
    /// offset by each source block's position in the concatenation.
    #[test]
    fn functor_restrict_offsets_merged_foreign_keys() {
        fn row(k: &str, v: &str) -> HashMap<String, Value> {
            let mut r = HashMap::new();
            r.insert(k.to_string(), Value::Str(v.into()));
            r
        }

        let edge = Edge {
            src: "a".into(),
            tgt: "b".into(),
            kind: "fk".into(),
            name: Some("link".into()),
        };
        let merged_edge = Edge {
            src: "t".into(),
            tgt: "t".into(),
            kind: "fk".into(),
            name: Some("link".into()),
        };

        let inst = FInstance::new()
            .with_table("a", vec![row("n", "a0"), row("n", "a1")])
            .with_table("b", vec![row("n", "b0"), row("n", "b1")])
            .with_foreign_key(edge.clone(), vec![(1, 0)]);

        let mut migration = CompiledMigration::default();
        migration.surviving_verts.insert("t".into());
        migration.vertex_remap.insert("a".into(), "t".into());
        migration.vertex_remap.insert("b".into(), "t".into());
        migration.edge_remap.insert(edge, merged_edge.clone());

        let restricted =
            functor_restrict(&inst, &migration).expect("functor_restrict should succeed");
        let rows = restricted.tables.get("t").expect("merged table present");
        assert_eq!(rows.len(), 4, "merged table concatenates both sources");

        let pairs = restricted
            .foreign_keys
            .get(&merged_edge)
            .expect("merged edge present");
        let (s, t) = pairs[0];
        assert_eq!(
            rows[s].get("n"),
            Some(&Value::Str("a1".into())),
            "FK source index must still address the `a` row it came from",
        );
        assert_eq!(
            rows[t].get("n"),
            Some(&Value::Str("b0".into())),
            "FK target index must be offset into the `b` block",
        );
    }

    /// Distinct source edges identified by `edge_remap` contribute a union of
    /// pairs, not a last-writer-wins overwrite, and the result is independent
    /// of hash iteration order.
    #[test]
    fn functor_restrict_unions_collided_edges_deterministically() {
        fn row(v: &str) -> HashMap<String, Value> {
            let mut r = HashMap::new();
            r.insert("n".to_string(), Value::Str(v.into()));
            r
        }
        fn self_edge(v: &str) -> Edge {
            Edge {
                src: v.into(),
                tgt: v.into(),
                kind: "fk".into(),
                name: Some("link".into()),
            }
        }

        let build = || {
            let inst = FInstance::new()
                .with_table("a", vec![row("a0"), row("a1")])
                .with_table("b", vec![row("b0"), row("b1")])
                .with_foreign_key(self_edge("a"), vec![(0, 1)])
                .with_foreign_key(self_edge("b"), vec![(0, 1)]);
            let mut migration = CompiledMigration::default();
            migration.surviving_verts.insert("t".into());
            migration.vertex_remap.insert("a".into(), "t".into());
            migration.vertex_remap.insert("b".into(), "t".into());
            migration.edge_remap.insert(self_edge("a"), self_edge("t"));
            migration.edge_remap.insert(self_edge("b"), self_edge("t"));
            functor_restrict(&inst, &migration).expect("functor_restrict should succeed")
        };

        let first = build();
        let pairs = first
            .foreign_keys
            .get(&self_edge("t"))
            .expect("merged edge present");
        assert_eq!(
            pairs,
            &vec![(0, 1), (2, 3)],
            "both source edges contribute their own offset pairs",
        );

        for _ in 0..16 {
            let again = build();
            assert_eq!(
                again.tables.get("t"),
                first.tables.get("t"),
                "merged table order must not depend on hash iteration order",
            );
            assert_eq!(
                again.foreign_keys.get(&self_edge("t")),
                first.foreign_keys.get(&self_edge("t")),
                "merged foreign keys must not depend on hash iteration order",
            );
        }
    }

    /// A migration merging `a` and `b` onto `t`.
    fn merging_migration() -> CompiledMigration {
        let mut vertex_remap = HashMap::new();
        vertex_remap.insert(Name::from("a"), Name::from("t"));
        vertex_remap.insert(Name::from("b"), Name::from("t"));
        CompiledMigration {
            surviving_verts: HashSet::from([Name::from("t")]),
            surviving_edges: HashSet::new(),
            vertex_remap,
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        }
    }

    /// What the direction of `functor_restrict` actually is, since its name
    /// says only how much of the schema it keeps.
    ///
    /// Over a merging vertex map it returns the fibre's coproduct in the
    /// target -- `Sigma_F`'s answer -- and not the source-indexed tables
    /// precomposition would return.
    #[test]
    fn functor_restrict_pushes_forward_rather_than_precomposing() {
        let m = merging_migration();
        let x = FInstance::new()
            .with_table("a", vec![numbered(1), numbered(2)])
            .with_table("b", vec![numbered(3)]);

        let restricted = functor_restrict(&x, &m).expect("restrict");
        assert_eq!(
            restricted.row_count("t"),
            3,
            "the merged fibre's rows are concatenated in the target",
        );
        assert_eq!(
            restricted.table_count(),
            1,
            "the result is indexed by the target's vertices",
        );

        let pushed = crate::adjunction::f_sigma(&x, &m).expect("sigma");
        assert_eq!(
            restricted.row_count("t"),
            pushed.row_count("t"),
            "restrict agrees with Sigma_F on the surviving fragment",
        );

        let precomposed = crate::adjunction::f_delta(&x, &m);
        assert_eq!(
            precomposed.table_count(),
            2,
            "precomposition is indexed by the source's vertices instead",
        );
        assert!(
            !precomposed.tables.contains_key("t"),
            "precomposition returns an S-instance, which has no table at `t`",
        );
    }

    /// A row carrying a single integer under `v`.
    fn numbered(value: i64) -> HashMap<String, Value> {
        HashMap::from([("v".to_owned(), Value::Int(value))])
    }
}
