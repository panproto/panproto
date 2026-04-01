//! Right Kan extension (`Pi_F`) for instances.
//!
//! The right Kan extension computes the "limit" (product) over fibers
//! of a migration morphism. For set-valued functor instances this means
//! taking Cartesian products of rows when multiple source vertices map
//! to the same target vertex. For W-type instances, only the simple
//! case (injective on vertices) is supported.

use std::collections::HashMap;

use panproto_gat::Name;
use panproto_schema::{Edge, Schema};

use crate::error::RestrictError;
use crate::functor::FInstance;
use crate::metadata::Node;
use crate::value::Value;
use crate::wtype::{CompiledMigration, WInstance, reconstruct_fans, resolve_edge};

/// Right Kan extension (`Pi_F`) for set-valued functor instances.
///
/// Computes the product over fibers of the migration morphism. For each
/// target vertex, the fiber is the set of source vertices that map to it.
/// Single-element fibers copy the table directly; multi-element fibers
/// compute the Cartesian product of rows with column union.
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

    let mut new_tables: HashMap<String, Vec<HashMap<String, Value>>> = HashMap::new();

    // Steps 2-4: Process each fiber
    for (tgt_vertex, src_vertices) in &fiber_map {
        // Collect tables for each source vertex in the fiber
        let mut fiber_tables: Vec<&Vec<HashMap<String, Value>>> = Vec::new();
        for src_v in src_vertices {
            if let Some(rows) = instance.tables.get(&**src_v) {
                if !rows.is_empty() {
                    fiber_tables.push(rows);
                }
            }
        }

        if fiber_tables.is_empty() {
            new_tables.insert(tgt_vertex.to_string(), Vec::new());
            continue;
        }

        if fiber_tables.len() == 1 {
            // Single-element fiber: copy directly
            new_tables.insert(tgt_vertex.to_string(), fiber_tables[0].clone());
            continue;
        }

        // Multi-element fiber: Cartesian product
        // Check product size first
        let product_size: usize = fiber_tables.iter().map(|t| t.len()).product();
        if product_size > max_product_size {
            return Err(RestrictError::ProductSizeExceeded {
                vertex: tgt_vertex.to_string(),
                actual: product_size,
                limit: max_product_size,
            });
        }

        // Compute Cartesian product with column union
        let mut product_rows: Vec<HashMap<String, Value>> = vec![HashMap::new()];
        for table in &fiber_tables {
            let mut new_product = Vec::with_capacity(product_rows.len() * table.len());
            for existing_row in &product_rows {
                for new_row in *table {
                    let mut merged = existing_row.clone();
                    for (col, val) in new_row {
                        merged.insert(col.clone(), val.clone());
                    }
                    new_product.push(merged);
                }
            }
            product_rows = new_product;
        }

        new_tables.insert(tgt_vertex.to_string(), product_rows);
    }

    // Step 5: Foreign keys for surviving edges
    let mut new_fks: HashMap<Edge, Vec<(usize, usize)>> = HashMap::new();
    for (edge, pairs) in &instance.foreign_keys {
        if let Some(new_edge) = migration.edge_remap.get(edge) {
            if new_tables.contains_key(&*new_edge.src) && new_tables.contains_key(&*new_edge.tgt) {
                new_fks.insert(new_edge.clone(), pairs.clone());
            }
        } else if migration.surviving_edges.contains(edge)
            && new_tables.contains_key(&*edge.src)
            && new_tables.contains_key(&*edge.tgt)
        {
            new_fks.insert(edge.clone(), pairs.clone());
        }
    }

    Ok(FInstance {
        tables: new_tables,
        foreign_keys: new_fks,
    })
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

/// Check that no multi-element fiber produces a product exceeding the limit.
fn check_fiber_product_size(
    fiber_map: &HashMap<Name, Vec<Name>>,
    instance: &WInstance,
    max_product_nodes: usize,
) -> Result<(), RestrictError> {
    for (tgt_v, src_vs) in fiber_map {
        if src_vs.len() > 1 {
            let fiber_node_counts: Vec<usize> = src_vs
                .iter()
                .map(|sv| {
                    instance
                        .nodes
                        .values()
                        .filter(|n| n.anchor == *sv)
                        .count()
                        .max(1)
                })
                .collect();
            let product_size: usize = fiber_node_counts.iter().product();
            if product_size > max_product_nodes {
                return Err(RestrictError::ProductSizeExceeded {
                    vertex: tgt_v.to_string(),
                    actual: product_size,
                    limit: max_product_nodes,
                });
            }
        }
    }
    Ok(())
}

/// Right Kan extension (`Pi_F`) for W-type instances.
///
/// For single-element fibers (each target vertex has exactly one source
/// vertex), this is equivalent to remapping anchors and edges. For
/// multi-element fibers, the product of subtrees is computed via Cartesian
/// product, which can be exponential in the number of fiber sources.
///
/// The `max_product_nodes` parameter bounds the total number of nodes in
/// the result to prevent exponential blowup. If the product would exceed
/// this limit, [`RestrictError::ProductSizeExceeded`] is returned.
///
/// # Errors
///
/// Returns [`RestrictError::ProductSizeExceeded`] if any target vertex
/// has a multi-element fiber whose subtree product would exceed
/// `max_product_nodes` total nodes in the result.
pub fn wtype_pi(
    instance: &WInstance,
    tgt_schema: &Schema,
    migration: &CompiledMigration,
    max_product_nodes: usize,
) -> Result<WInstance, RestrictError> {
    let fiber_map = build_fiber_map(migration);
    check_fiber_product_size(&fiber_map, instance, max_product_nodes)?;

    // With single-element fibers, this is equivalent to wtype_extend
    // but we implement it directly for clarity.

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
            continue;
        }
        // Apply field transforms (coercions) to the Pi node.
        if let Some(transforms) = migration.field_transforms.get(&node.anchor) {
            let scalars = crate::wtype::collect_scalar_child_values(instance, id);
            crate::wtype::apply_field_transforms(&mut new_node, transforms, &scalars);
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
        };

        let result = wtype_pi(&inst, &schema, &migration, 10_000).unwrap();
        assert_eq!(result.node_count(), 2);
        assert_eq!(result.arc_count(), 1);
    }

    #[test]
    fn wtype_pi_rejects_large_multi_fiber() {
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
        };

        // With a very small limit, the product is rejected
        let result = wtype_pi(&inst, &schema, &migration, 0);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            RestrictError::ProductSizeExceeded { .. }
        ));
    }

    #[test]
    fn wtype_pi_accepts_small_multi_fiber() {
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
        };

        // With a generous limit, it succeeds
        let result = wtype_pi(&inst, &schema, &migration, 10_000);
        assert!(result.is_ok());
    }
}
