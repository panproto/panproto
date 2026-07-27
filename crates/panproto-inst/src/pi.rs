//! Right Kan extension (`Pi_F`) for instances.
//!
//! The right Kan extension computes the "limit" (product) over fibers
//! of a migration morphism. For set-valued functor instances this means
//! taking Cartesian products of rows when multiple source vertices map
//! to the same target vertex. For W-type instances, only vertex-injective
//! migrations are supported; a non-injective map is rejected rather than
//! silently producing `Sigma`-shaped output mislabeled as `Pi`.

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_schema::{Edge, Schema};

use crate::error::RestrictError;
use crate::functor::FInstance;
use crate::metadata::Node;
use crate::value::Value;
use crate::wtype::{CompiledMigration, WInstance, reconstruct_fans, resolve_edge};

/// A functor table: a list of rows, each mapping column name to value.
type FiberRows = Vec<HashMap<String, Value>>;

/// Per-row provenance for a target vertex's rows: for each row, the source row
/// index drawn from each fiber-component source vertex.
type FiberProvenance = Vec<HashMap<Name, usize>>;

/// Right Kan extension (`Pi_F`) for set-valued functor instances.
///
/// Computes the product over fibers of the migration morphism. For each
/// target vertex, the fiber is the set of source vertices that map to it.
/// Single-element fibers copy the table directly; multi-element fibers
/// compute the Cartesian product of rows with column union.
///
/// Foreign keys are carried through the product. Each product row records
/// which source row it drew from every fiber component, and each original FK
/// pair `(i, j)` on edge `e` is remapped to every product row whose component
/// index for `e`'s endpoint vertex equals the original index. Thus an FK
/// touching a product-side vertex is preserved (fanned out across the product
/// rows), not dropped.
///
/// # Errors
///
/// Returns [`RestrictError::ProductSizeExceeded`] if a Cartesian product
/// exceeds `max_product_size`, or other `RestrictError` variants for
/// structural issues.
pub fn functor_pi(
    instance: &FInstance,
    migration: &CompiledMigration,
    max_product_size: usize,
) -> Result<FInstance, RestrictError> {
    // Step 1: Build fiber map. For each target vertex, collect source vertices
    let mut fiber_map: HashMap<Name, Vec<Name>> = HashMap::new();

    // Collect all remap targets so we can distinguish target-only vertices
    let remap_targets: std::collections::HashSet<&Name> = migration.vertex_remap.values().collect();

    // Vertices that are remapped
    for (src, tgt) in &migration.vertex_remap {
        fiber_map.entry(tgt.clone()).or_default().push(src.clone());
    }

    // Vertices that survive without remap (identity mapping).
    // Only add if the vertex is not a remap source (key) AND not
    // exclusively a remap target (i.e., it maps to itself as a source).
    for sv in &migration.surviving_verts {
        if !migration.vertex_remap.contains_key(sv) && !remap_targets.contains(sv) {
            fiber_map.entry(sv.clone()).or_default().push(sv.clone());
        }
    }

    let mut new_tables: HashMap<String, FiberRows> = HashMap::new();
    // For each target vertex, the provenance of each of its rows: a map from
    // source vertex to the source-table row index that this (possibly product)
    // row was built from. Parallel to `new_tables[tgt]`. Used to remap foreign
    // keys through the Cartesian product below.
    let mut row_provenance: HashMap<Name, FiberProvenance> = HashMap::new();

    // Steps 2-4: Process each fiber, building its (possibly product) table and
    // the per-row provenance used to remap foreign keys below.
    for (tgt_vertex, src_vertices) in &fiber_map {
        let (rows, provenance) =
            build_fiber_table(instance, src_vertices, tgt_vertex, max_product_size)?;
        new_tables.insert(tgt_vertex.to_string(), rows);
        row_provenance.insert(tgt_vertex.clone(), provenance);
    }

    // Step 5: Foreign keys for surviving edges, remapped through the product.
    // The original edge's endpoints are source vertices; each FK pair (i, j)
    // is emitted for every product row of the new source endpoint whose
    // component index for the original source vertex equals i, crossed with
    // every product row of the new target endpoint matching j.
    let mut new_fks: HashMap<Edge, Vec<(usize, usize)>> = HashMap::new();
    for (edge, pairs) in &instance.foreign_keys {
        let new_edge = if let Some(remapped) = migration.edge_remap.get(edge) {
            remapped.clone()
        } else if migration.surviving_edges.contains(edge) {
            edge.clone()
        } else {
            continue;
        };

        if !new_tables.contains_key(&*new_edge.src) || !new_tables.contains_key(&*new_edge.tgt) {
            continue;
        }

        let src_prov = row_provenance.get(&new_edge.src);
        let tgt_prov = row_provenance.get(&new_edge.tgt);

        let mut remapped_pairs: Vec<(usize, usize)> = Vec::new();
        for &(i, j) in pairs {
            let src_rows = product_rows_for(src_prov, &edge.src, i);
            let tgt_rows = product_rows_for(tgt_prov, &edge.tgt, j);
            for &p in &src_rows {
                for &q in &tgt_rows {
                    remapped_pairs.push((p, q));
                }
            }
        }

        if !remapped_pairs.is_empty() {
            new_fks.insert(new_edge, remapped_pairs);
        }
    }

    Ok(FInstance {
        tables: new_tables,
        foreign_keys: new_fks,
    })
}

/// Build the table and per-row provenance for one target vertex's fiber.
///
/// Single-element fibers copy the source table directly with identity
/// provenance (row `p` came from source row `p`); multi-element fibers form the
/// Cartesian product with column union, recording for each product row the
/// source row index it drew from every fiber-component source vertex.
///
/// # Errors
///
/// Returns [`RestrictError::ProductSizeExceeded`] if a multi-element fiber's
/// Cartesian product would exceed `max_product_size`.
fn build_fiber_table(
    instance: &FInstance,
    src_vertices: &[Name],
    tgt_vertex: &Name,
    max_product_size: usize,
) -> Result<(FiberRows, FiberProvenance), RestrictError> {
    // Collect (source vertex, rows) for each non-empty source table so the
    // product can be traced back to the contributing source rows.
    let mut fiber_sources: Vec<(&Name, &FiberRows)> = Vec::new();
    for src_v in src_vertices {
        if let Some(rows) = instance.tables.get(&**src_v) {
            if !rows.is_empty() {
                fiber_sources.push((src_v, rows));
            }
        }
    }

    if fiber_sources.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }

    if fiber_sources.len() == 1 {
        // Single-element fiber: copy directly (fast path). Provenance is the
        // identity: row `p` came from source row `p`.
        let (src_v, rows) = fiber_sources[0];
        let provenance = (0..rows.len())
            .map(|i| {
                let mut m = HashMap::new();
                m.insert((*src_v).clone(), i);
                m
            })
            .collect();
        return Ok((rows.clone(), provenance));
    }

    // Multi-element fiber: Cartesian product. Check product size first.
    let product_size: usize = fiber_sources.iter().map(|(_, t)| t.len()).product();
    if product_size > max_product_size {
        return Err(RestrictError::ProductSizeExceeded {
            vertex: tgt_vertex.to_string(),
            actual: product_size,
            limit: max_product_size,
        });
    }

    // Compute the product with column union, tracking for each product row the
    // contributing source row index per source vertex.
    let mut product_rows: FiberRows = vec![HashMap::new()];
    let mut product_prov: FiberProvenance = vec![HashMap::new()];
    for (src_v, table) in &fiber_sources {
        let mut new_product = Vec::with_capacity(product_rows.len() * table.len());
        let mut new_prov = Vec::with_capacity(product_rows.len() * table.len());
        for (existing_row, existing_prov) in product_rows.iter().zip(&product_prov) {
            for (row_idx, new_row) in table.iter().enumerate() {
                let mut merged = existing_row.clone();
                for (col, val) in new_row {
                    // Attribute discipline: a column already contributed by an
                    // earlier fiber source must agree, or the product-row merge
                    // would silently overwrite one value with another.
                    if let Some(existing) = merged.get(col) {
                        if existing != val {
                            return Err(RestrictError::AttributeCollision {
                                vertex: tgt_vertex.to_string(),
                                column: col.clone(),
                            });
                        }
                    }
                    merged.insert(col.clone(), val.clone());
                }
                new_product.push(merged);
                let mut prov = existing_prov.clone();
                prov.insert((*src_v).clone(), row_idx);
                new_prov.push(prov);
            }
        }
        product_rows = new_product;
        product_prov = new_prov;
    }

    Ok((product_rows, product_prov))
}

/// Product rows of a target vertex that drew source row `source_row` from
/// `source_vertex`.
///
/// `provenance` is the per-row component-index map for a target vertex (as
/// built in [`functor_pi`]). Returns the indices of every row whose component
/// index for `source_vertex` equals `source_row`. For a single-element fiber
/// this is at most the singleton `[source_row]`; for a product it is every
/// product row that used that source row. Returns empty when there is no
/// provenance (e.g. the source vertex is not part of the target's fiber).
fn product_rows_for(
    provenance: Option<&FiberProvenance>,
    source_vertex: &Name,
    source_row: usize,
) -> Vec<usize> {
    let Some(prov) = provenance else {
        return Vec::new();
    };
    prov.iter()
        .enumerate()
        .filter_map(|(p, comp)| (comp.get(source_vertex) == Some(&source_row)).then_some(p))
        .collect()
}

/// Build the fiber map from a migration's vertex remap and surviving vertices.
fn build_fiber_map(migration: &CompiledMigration) -> HashMap<Name, Vec<Name>> {
    let mut fiber_map: HashMap<Name, Vec<Name>> = HashMap::new();
    let remap_targets: std::collections::HashSet<&Name> = migration.vertex_remap.values().collect();

    for (src, tgt) in &migration.vertex_remap {
        fiber_map.entry(tgt.clone()).or_default().push(src.clone());
    }

    for sv in &migration.surviving_verts {
        if !migration.vertex_remap.contains_key(sv) && !remap_targets.contains(sv) {
            fiber_map.entry(sv.clone()).or_default().push(sv.clone());
        }
    }

    fiber_map
}

/// Right Kan extension (`Pi_F`) for W-type instances — vertex-injective only.
///
/// This function is defined only for migrations that are injective on
/// vertices: each target vertex has exactly one source vertex in its fiber.
/// Under that restriction the extension is a relabeling — it remaps anchors
/// and edges, preserving the tree structure — and constructs no product.
///
/// A non-injective migration (two or more source vertices mapping to one
/// target vertex) would require building a product of subtrees, which is not
/// implemented here; see [`functor_pi`] for the set-valued product. Rather
/// than silently emit `Sigma`-shaped output mislabeled as `Pi`, this
/// function rejects such a migration with
/// [`RestrictError::NonInjectiveVertexMap`].
///
/// Like the total left Kan extension, every source node's anchor must be
/// remapped or surviving; an unmapped anchor is reported via
/// [`RestrictError::UnmappedAnchor`] instead of being dropped silently.
///
/// The `max_product_nodes` parameter is retained for signature compatibility
/// with the set-valued [`functor_pi`] path; because the vertex-injective case
/// never forms a product, it imposes no bound here.
///
/// # Errors
///
/// - [`RestrictError::NonInjectiveVertexMap`] if two or more source vertices
///   map to the same target vertex.
/// - [`RestrictError::UnmappedAnchor`] if a source node's anchor is neither
///   remapped nor surviving.
/// - [`RestrictError::RootPruned`] if the root cannot be mapped, or another
///   [`RestrictError`] variant if edge resolution fails.
pub fn wtype_pi(
    instance: &WInstance,
    tgt_schema: &Schema,
    migration: &CompiledMigration,
    max_product_nodes: usize,
) -> Result<WInstance, RestrictError> {
    let fiber_map = build_fiber_map(migration);

    // Pi over W-types is defined here only for vertex-injective migrations:
    // each target vertex must have exactly one source vertex in its fiber. A
    // fiber with multiple sources would require a product of subtrees, which
    // this function does not build (see `functor_pi` for the set-valued
    // product). Reject rather than silently return Sigma-shaped output.
    for (tgt, srcs) in &fiber_map {
        if srcs.len() > 1 {
            let mut sources = srcs.clone();
            sources.sort_unstable();
            return Err(RestrictError::NonInjectiveVertexMap {
                target: tgt.clone(),
                sources,
            });
        }
    }

    // The vertex-injective case never forms a product, so `max_product_nodes`
    // imposes no bound; it is kept only for signature compatibility with the
    // set-valued `functor_pi` path.
    let _ = max_product_nodes;

    let root_node = instance
        .nodes
        .get(&instance.root)
        .ok_or(RestrictError::RootPruned)?;

    let root_anchor = &root_node.anchor;
    if !migration.surviving_verts.contains(root_anchor)
        && !migration.vertex_remap.contains_key(root_anchor)
    {
        return Err(RestrictError::RootPruned);
    }

    // Remap nodes
    let mut new_nodes: HashMap<u32, Node> = HashMap::with_capacity(instance.nodes.len());
    for (&id, node) in &instance.nodes {
        let mut new_node = node.clone();
        if let Some(remapped) = migration.vertex_remap.get(&node.anchor) {
            new_node.anchor.clone_from(remapped);
        } else if !migration.surviving_verts.contains(&node.anchor) {
            // No image in the target schema; a total Pi cannot map this node.
            return Err(RestrictError::UnmappedAnchor {
                anchor: node.anchor.clone(),
                node_id: id,
            });
        }
        // Apply value transforms (coercions and op-to-term assignments) to
        // the Pi node.
        let transforms = migration.value_transforms(&node.anchor);
        if !transforms.is_empty() {
            let ctx = crate::wtype::TransformContext::new(None, instance, id, &transforms);
            crate::wtype::apply_field_transforms(&mut new_node, &transforms, &ctx)?;
        }
        new_nodes.insert(id, new_node);
    }

    // Remap arcs
    let mut new_arcs: Vec<(u32, u32, Edge)> = Vec::with_capacity(instance.arcs.len());
    for &(parent, child, ref edge) in &instance.arcs {
        if !new_nodes.contains_key(&parent) || !new_nodes.contains_key(&child) {
            continue;
        }

        if let Some(new_edge) = migration.edge_remap.get(edge) {
            new_arcs.push((parent, child, new_edge.clone()));
        } else if migration.surviving_edges.contains(edge) {
            let parent_anchor = &new_nodes[&parent].anchor;
            let child_anchor = &new_nodes[&child].anchor;
            if edge.src == *parent_anchor && edge.tgt == *child_anchor {
                new_arcs.push((parent, child, edge.clone()));
            } else {
                let resolved =
                    resolve_edge(tgt_schema, &migration.resolver, parent_anchor, child_anchor)?;
                new_arcs.push((parent, child, resolved));
            }
        } else {
            let parent_anchor = &new_nodes[&parent].anchor;
            let child_anchor = &new_nodes[&child].anchor;
            let resolved =
                resolve_edge(tgt_schema, &migration.resolver, parent_anchor, child_anchor)?;
            new_arcs.push((parent, child, resolved));
        }
    }

    // Reconstruct fans
    let surviving_ids: rustc_hash::FxHashSet<u32> = new_nodes.keys().copied().collect();
    let empty_ancestors = rustc_hash::FxHashMap::default();
    let new_fans = reconstruct_fans(
        instance,
        &surviving_ids,
        &empty_ancestors,
        migration,
        tgt_schema,
    )?;

    let new_schema_root = migration
        .vertex_remap
        .get(&instance.schema_root)
        .cloned()
        .unwrap_or_else(|| instance.schema_root.clone());

    Ok(WInstance::new(
        new_nodes,
        new_arcs,
        new_fans,
        instance.root,
        new_schema_root,
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::collections::HashSet;

    use panproto_schema::Vertex;
    use smallvec::smallvec;

    use super::*;

    fn make_test_schema(vertices: &[&str], edges: &[Edge]) -> Schema {
        let mut between = HashMap::new();
        for edge in edges {
            between
                .entry((Name::from(&*edge.src), Name::from(&*edge.tgt)))
                .or_insert_with(|| smallvec![])
                .push(edge.clone());
        }
        Schema {
            protocol: "test".into(),
            vertices: vertices
                .iter()
                .map(|&v| {
                    (
                        Name::from(v),
                        Vertex {
                            id: Name::from(v),
                            kind: Name::from("object"),
                            nsid: None,
                        },
                    )
                })
                .collect(),
            edges: HashMap::new(),
            hyper_edges: HashMap::new(),
            constraints: HashMap::new(),
            required: HashMap::new(),
            nsids: HashMap::new(),
            entries: Vec::new(),
            variants: HashMap::new(),
            orderings: HashMap::new(),
            recursion_points: HashMap::new(),
            spans: HashMap::new(),
            usage_modes: HashMap::new(),
            nominal: HashMap::new(),
            coercions: HashMap::new(),
            mergers: HashMap::new(),
            defaults: HashMap::new(),
            policies: HashMap::new(),
            outgoing: HashMap::new(),
            incoming: HashMap::new(),
            between,
        }
    }

    // --- functor_pi tests ---

    #[test]
    fn functor_pi_single_fiber_copies_table() {
        let mut row = HashMap::new();
        row.insert("name".to_string(), Value::Str("Alice".into()));
        let inst = FInstance::new().with_table("users", vec![row.clone()]);

        let migration = CompiledMigration {
            surviving_verts: HashSet::from([Name::from("users")]),
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

        let result = functor_pi(&inst, &migration, 100).unwrap();
        assert_eq!(result.table_count(), 1);
        assert_eq!(result.row_count("users"), 1);
    }

    #[test]
    fn functor_pi_multi_fiber_cartesian_product() {
        let rows_a = vec![
            {
                let mut r = HashMap::new();
                r.insert("x".to_string(), Value::Int(1));
                r
            },
            {
                let mut r = HashMap::new();
                r.insert("x".to_string(), Value::Int(2));
                r
            },
        ];
        let rows_b = vec![{
            let mut r = HashMap::new();
            r.insert("y".to_string(), Value::Int(10));
            r
        }];
        let inst = FInstance::new()
            .with_table("src_a", rows_a)
            .with_table("src_b", rows_b);

        let mut vertex_remap = HashMap::new();
        vertex_remap.insert(Name::from("src_a"), Name::from("merged"));
        vertex_remap.insert(Name::from("src_b"), Name::from("merged"));

        let migration = CompiledMigration {
            surviving_verts: HashSet::from([Name::from("merged")]),
            surviving_edges: HashSet::new(),
            vertex_remap,
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        };

        let result = functor_pi(&inst, &migration, 100).unwrap();
        // 2 * 1 = 2 product rows
        assert_eq!(result.row_count("merged"), 2);
        // Each product row should have both x and y columns
        let merged_rows = result.tables.get("merged").unwrap();
        for row in merged_rows {
            assert!(row.contains_key("x"));
            assert!(row.contains_key("y"));
        }
    }

    #[test]
    fn functor_pi_product_size_exceeded() {
        let rows_a = vec![
            {
                let mut r = HashMap::new();
                r.insert("x".to_string(), Value::Int(1));
                r
            },
            {
                let mut r = HashMap::new();
                r.insert("x".to_string(), Value::Int(2));
                r
            },
        ];
        let rows_b = vec![
            {
                let mut r = HashMap::new();
                r.insert("y".to_string(), Value::Int(10));
                r
            },
            {
                let mut r = HashMap::new();
                r.insert("y".to_string(), Value::Int(20));
                r
            },
        ];
        let inst = FInstance::new()
            .with_table("src_a", rows_a)
            .with_table("src_b", rows_b);

        let mut vertex_remap = HashMap::new();
        vertex_remap.insert(Name::from("src_a"), Name::from("merged"));
        vertex_remap.insert(Name::from("src_b"), Name::from("merged"));

        let migration = CompiledMigration {
            surviving_verts: HashSet::from([Name::from("merged")]),
            surviving_edges: HashSet::new(),
            vertex_remap,
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        };

        // Limit to 2 but product would be 4
        let result = functor_pi(&inst, &migration, 2);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            matches!(
                err,
                RestrictError::ProductSizeExceeded {
                    actual: 4,
                    limit: 2,
                    ..
                }
            ),
            "expected ProductSizeExceeded, got {err:?}"
        );
    }

    #[test]
    fn functor_pi_preserves_fk_from_product_side_to_untouched() {
        // src_a and src_c both map to `merged` (a two-element fiber → product);
        // `other` is untouched. An FK from src_a to `other` must survive,
        // remapped onto the product rows of `merged`.
        let rows_a = vec![
            HashMap::from([("a".to_string(), Value::Int(0))]),
            HashMap::from([("a".to_string(), Value::Int(1))]),
        ];
        let rows_c = vec![HashMap::from([("c".to_string(), Value::Int(0))])];
        let rows_o = vec![
            HashMap::from([("b".to_string(), Value::Int(0))]),
            HashMap::from([("b".to_string(), Value::Int(1))]),
        ];

        let fk_edge = Edge {
            src: "src_a".into(),
            tgt: "other".into(),
            kind: "ref".into(),
            name: Some("link".into()),
        };
        let inst = FInstance::new()
            .with_table("src_a", rows_a)
            .with_table("src_c", rows_c)
            .with_table("other", rows_o)
            // src_a row 1 references other row 0.
            .with_foreign_key(fk_edge.clone(), vec![(1, 0)]);

        let mut vertex_remap = HashMap::new();
        vertex_remap.insert(Name::from("src_a"), Name::from("merged"));
        vertex_remap.insert(Name::from("src_c"), Name::from("merged"));

        let new_edge = Edge {
            src: "merged".into(),
            tgt: "other".into(),
            kind: "ref".into(),
            name: Some("link".into()),
        };
        let mut edge_remap = HashMap::new();
        edge_remap.insert(fk_edge, new_edge.clone());

        let migration = CompiledMigration {
            surviving_verts: HashSet::from([Name::from("merged"), Name::from("other")]),
            surviving_edges: HashSet::new(),
            vertex_remap,
            edge_remap,
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        };

        let result = functor_pi(&inst, &migration, 100).unwrap();
        assert_eq!(result.row_count("merged"), 2);
        assert_eq!(result.row_count("other"), 2);

        // src_a row 1 lives at `merged` product row 1 (src_c has a single row),
        // and `other` row 0 is unchanged.
        let expected: Vec<(usize, usize)> = vec![(1, 0)];
        assert_eq!(result.foreign_keys.get(&new_edge), Some(&expected));
    }

    #[test]
    fn functor_pi_preserves_fk_between_two_product_sides() {
        // src_a + src_a2 → m1 (product); src_b + src_b2 → m2 (product). An FK
        // from src_a to src_b — both product-side — must survive, remapped onto
        // the product rows of m1 and m2.
        let rows_a = vec![
            HashMap::from([("a".to_string(), Value::Int(0))]),
            HashMap::from([("a".to_string(), Value::Int(1))]),
        ];
        let rows_p = vec![HashMap::from([("a2".to_string(), Value::Int(0))])];
        let rows_b = vec![
            HashMap::from([("b".to_string(), Value::Int(0))]),
            HashMap::from([("b".to_string(), Value::Int(1))]),
        ];
        let rows_q = vec![HashMap::from([("b2".to_string(), Value::Int(0))])];

        let fk_edge = Edge {
            src: "src_a".into(),
            tgt: "src_b".into(),
            kind: "ref".into(),
            name: Some("link".into()),
        };
        let inst = FInstance::new()
            .with_table("src_a", rows_a)
            .with_table("src_a2", rows_p)
            .with_table("src_b", rows_b)
            .with_table("src_b2", rows_q)
            // src_a row 0 references src_b row 1.
            .with_foreign_key(fk_edge.clone(), vec![(0, 1)]);

        let mut vertex_remap = HashMap::new();
        vertex_remap.insert(Name::from("src_a"), Name::from("m1"));
        vertex_remap.insert(Name::from("src_a2"), Name::from("m1"));
        vertex_remap.insert(Name::from("src_b"), Name::from("m2"));
        vertex_remap.insert(Name::from("src_b2"), Name::from("m2"));

        let new_edge = Edge {
            src: "m1".into(),
            tgt: "m2".into(),
            kind: "ref".into(),
            name: Some("link".into()),
        };
        let mut edge_remap = HashMap::new();
        edge_remap.insert(fk_edge, new_edge.clone());

        let migration = CompiledMigration {
            surviving_verts: HashSet::from([Name::from("m1"), Name::from("m2")]),
            surviving_edges: HashSet::new(),
            vertex_remap,
            edge_remap,
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        };

        let result = functor_pi(&inst, &migration, 100).unwrap();
        assert_eq!(result.row_count("m1"), 2);
        assert_eq!(result.row_count("m2"), 2);

        // src_a row 0 → m1 product row 0; src_b row 1 → m2 product row 1
        // (the secondary source tables each have a single row).
        let expected: Vec<(usize, usize)> = vec![(0, 1)];
        assert_eq!(result.foreign_keys.get(&new_edge), Some(&expected));
    }

    // --- wtype_pi tests ---

    #[test]
    fn wtype_pi_identity_migration() {
        let edge = Edge {
            src: "root".into(),
            tgt: "leaf".into(),
            kind: "prop".into(),
            name: Some("child".into()),
        };
        let schema = make_test_schema(&["root", "leaf"], std::slice::from_ref(&edge));

        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, "root"));
        nodes.insert(1, Node::new(1, "leaf"));
        let arcs = vec![(0, 1, edge.clone())];
        let inst = WInstance::new(nodes, arcs, vec![], 0, Name::from("root"));

        let migration = CompiledMigration {
            surviving_verts: HashSet::from([Name::from("root"), Name::from("leaf")]),
            surviving_edges: HashSet::from([edge]),
            vertex_remap: HashMap::new(),
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        };

        let result = wtype_pi(&inst, &schema, &migration, 10_000).unwrap();
        assert_eq!(result.node_count(), 2);
        assert_eq!(result.arc_count(), 1);
    }

    #[test]
    fn wtype_pi_rejects_non_injective_vertex_map() {
        let schema = make_test_schema(&["merged"], &[]);

        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, "src_a"));
        let inst = WInstance::new(nodes, vec![], vec![], 0, Name::from("src_a"));

        let mut vertex_remap = HashMap::new();
        vertex_remap.insert(Name::from("src_a"), Name::from("merged"));
        vertex_remap.insert(Name::from("src_b"), Name::from("merged"));

        let migration = CompiledMigration {
            surviving_verts: HashSet::from([Name::from("merged")]),
            surviving_edges: HashSet::new(),
            vertex_remap,
            edge_remap: HashMap::new(),
            resolver: HashMap::new(),
            hyper_resolver: HashMap::new(),
            field_transforms: HashMap::new(),
            conditional_survival: HashMap::new(),
            op_term_assignments: HashMap::new(),
            expansion_path: HashMap::new(),
        };

        // src_a and src_b both map to `merged`: a non-injective vertex map,
        // which wtype_pi rejects rather than producing Sigma-shaped output.
        // The bound argument is irrelevant to this outcome.
        let err = wtype_pi(&inst, &schema, &migration, 10_000).unwrap_err();
        match err {
            RestrictError::NonInjectiveVertexMap { target, sources } => {
                assert_eq!(target, Name::from("merged"));
                assert_eq!(sources, vec![Name::from("src_a"), Name::from("src_b")]);
            }
            other => panic!("expected NonInjectiveVertexMap, got {other:?}"),
        }
    }

    #[test]
    fn wtype_pi_errors_on_unmapped_anchor() {
        let schema = make_test_schema(&["root"], &[]);

        let mut nodes = HashMap::new();
        nodes.insert(0, Node::new(0, "root"));
        nodes.insert(1, Node::new(1, "orphan"));
        let edge = Edge {
            src: "root".into(),
            tgt: "orphan".into(),
            kind: "prop".into(),
            name: Some("x".into()),
        };
        let inst = WInstance::new(nodes, vec![(0, 1, edge)], vec![], 0, Name::from("root"));

        // `root` survives; `orphan` is neither remapped nor surviving, so it
        // has no image in the target and a total Pi must report it.
        let migration = CompiledMigration {
            surviving_verts: HashSet::from([Name::from("root")]),
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

        let err = wtype_pi(&inst, &schema, &migration, 10_000).unwrap_err();
        assert!(
            matches!(err, RestrictError::UnmappedAnchor { node_id: 1, .. }),
            "expected UnmappedAnchor for node 1, got {err:?}"
        );
    }
}
