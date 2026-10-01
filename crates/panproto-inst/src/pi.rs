//! Right Kan extension (`Pi_F`) for instances.
//!
//! The right Kan extension computes the "limit" (product) over fibers
//! of a migration morphism. For set-valued functor instances this means
//! taking Cartesian products of rows when multiple source vertices map
//! to the same target vertex. For W-type instances, only vertex-injective
//! migrations are supported; a non-injective map is rejected rather than
//! silently producing `Sigma`-shaped output mislabeled as `Pi`.

use std::collections::{BTreeMap, HashMap};

use panproto_gat::Name;
use panproto_schema::{Edge, Schema};

use crate::error::RestrictError;
use crate::functor::{FInstance, Row, allocate_row, row_allocator};
use crate::instance_hom::FInstanceHom;
use crate::metadata::Node;
use crate::view_ref::RowId;
use crate::wtype::{CompiledMigration, WInstance, reconstruct_fans, resolve_edge};

/// The rows of a target vertex, each with the source row it drew from every
/// fibre component.
type FiberRows = Vec<(Row, BTreeMap<Name, RowId>)>;

/// The rows placed in one target table, each with the source row it drew
/// from every fibre component.
type PlacedRows = Vec<(RowId, BTreeMap<Name, RowId>)>;

/// Right Kan extension (`Pi_F`) for set-valued functor instances.
///
/// The first projection of [`functor_pi_with_hom`]; see there.
///
/// # Errors
///
/// As [`functor_pi_with_hom`].
pub fn functor_pi(
    instance: &FInstance,
    src_schema: &Schema,
    tgt_schema: &Schema,
    migration: &CompiledMigration,
    max_product_size: usize,
) -> Result<FInstance, RestrictError> {
    functor_pi_with_hom(
        instance,
        src_schema,
        tgt_schema,
        migration,
        max_product_size,
    )
    .map(|(pi, _)| pi)
}

/// Right Kan extension (`Pi_F`) for set-valued functor instances, with the
/// projection of every row of the result onto each fibre component.
///
/// For each target vertex, the fibre is the set of source vertices that map
/// to it, and the target's table is the limit over the fibre: the Cartesian
/// product of the component tables with column union, equalised along every
/// internal arrow. An internal arrow is an edge of `src_schema` whose
/// endpoints both lie in the fibre and which the migration carries to no
/// target edge, so `F` sends it to the identity of the target vertex; a
/// product row is kept only when its components are a pair of that foreign
/// key. The arrows are read from the schema, not from the instance, so an
/// internal foreign key without pairs is the empty relation and empties the
/// limit. A fibre
/// component whose table is empty or absent is an empty factor, so the
/// target's table is empty; the empty fibre is the empty product, whose limit
/// holds one row with no cells.
///
/// Row ids: a fibre with one component and no internal arrow keeps its rows'
/// ids (the limit is the component itself); every other row of the result is
/// fresh, allocated from the least id above every row of the input, the
/// target vertices taken in ascending name and each target's rows in the
/// product's order (components in ascending name, the last varying fastest).
///
/// Foreign keys along carried edges travel through the product: a pair
/// `(i, j)` of source edge `e` relates every result row drawing `i` from
/// `e`'s source to every result row drawing `j` from its target, the largest
/// relation the projections respect.
///
/// The returned map sends each source vertex `s` of a fibre to the
/// projection of its target's rows onto the rows of `s`, as an
/// [`FInstanceHom`] whose table map sends the target table `F(s)` to `s`.
///
/// # Errors
///
/// Returns [`RestrictError::NoEdgeFound`] when the migration carries an edge
/// of `src_schema` to an edge `tgt_schema` does not have,
/// [`RestrictError::ProductSizeExceeded`] if a Cartesian product exceeds
/// `max_product_size`, [`RestrictError::AttributeCollision`] when two
/// components of one product row disagree on a shared column, and
/// [`RestrictError::RowIdsExhausted`] when the fresh row ids run out.
pub fn functor_pi_with_hom(
    instance: &FInstance,
    src_schema: &Schema,
    tgt_schema: &Schema,
    migration: &CompiledMigration,
    max_product_size: usize,
) -> Result<(FInstance, BTreeMap<Name, FInstanceHom>), RestrictError> {
    check_carried_edges(src_schema, tgt_schema, migration)?;
    let fiber_map = build_fiber_map(migration);
    let mut allocator = row_allocator(instance.row_ids());

    let mut out = FInstance::new();
    let mut projections: BTreeMap<Name, FInstanceHom> = BTreeMap::new();
    let mut provenance: HashMap<Name, PlacedRows> = HashMap::new();

    for (tgt_vertex, src_vertices) in &fiber_map {
        let internal = internal_arrows(src_schema, migration, src_vertices);
        let rows = build_fiber_table(
            instance,
            src_vertices,
            tgt_vertex,
            &internal,
            max_product_size,
        )?;
        let keeps_ids = src_vertices.len() == 1 && internal.is_empty();
        let mut placed = Vec::with_capacity(rows.len());
        for (row, components) in rows {
            let id = match components.values().next() {
                Some(&own) if keeps_ids => own,
                _ => allocate_row(&mut allocator).map_err(|_| RestrictError::RowIdsExhausted)?,
            };
            for (src, &component) in &components {
                projections
                    .entry(src.clone())
                    .or_default()
                    .row_map
                    .insert(id, component);
            }
            out.place_row(tgt_vertex.clone(), id, row);
            placed.push((id, components));
        }
        provenance.insert(tgt_vertex.clone(), placed);
    }
    let fibre_of: HashMap<&Name, &Name> = fiber_map
        .iter()
        .flat_map(|(tgt, fibre)| fibre.iter().map(move |src| (src, tgt)))
        .collect();
    let projections = projections
        .into_iter()
        .map(|(src, hom)| {
            let table_map = fibre_of
                .get(&src)
                .map(|&tgt| (tgt.clone(), src.clone()))
                .into_iter()
                .collect();
            (src, hom.with_table_map(table_map))
        })
        .collect();

    // The result rows drawn from each source row, per target table and
    // fibre component, so carrying a pair reads only the rows it relates.
    let mut drawn_from: HashMap<(&Name, &Name, RowId), Vec<RowId>> = HashMap::new();
    for (tgt_vertex, placed) in &provenance {
        for (id, components) in placed {
            for (component, &src_row) in components {
                drawn_from
                    .entry((tgt_vertex, component, src_row))
                    .or_default()
                    .push(*id);
            }
        }
    }
    for (edge, pairs) in instance.foreign_keys() {
        let Some(new_edge) = carried_edge(migration, edge).cloned() else {
            continue;
        };
        for &(i, j) in pairs {
            let (Some(sources), Some(targets)) = (
                drawn_from.get(&(&new_edge.src, &edge.src, i)),
                drawn_from.get(&(&new_edge.tgt, &edge.tgt, j)),
            ) else {
                continue;
            };
            for &p in sources {
                for &q in targets {
                    out.place_pair(new_edge.clone(), p, q);
                }
            }
        }
    }

    Ok((out, projections))
}

/// The edge a source edge travels to under a migration: its remap when the
/// migration names one, itself when it survives, and `None` otherwise.
fn carried_edge<'a>(migration: &'a CompiledMigration, edge: &'a Edge) -> Option<&'a Edge> {
    migration
        .edge_remap
        .get(edge)
        .or_else(|| migration.surviving_edges.contains(edge).then_some(edge))
}

/// Check that every edge of `src` the migration carries lands on an edge of
/// `tgt`, taking the edges in ascending order.
fn check_carried_edges(
    src: &Schema,
    tgt: &Schema,
    migration: &CompiledMigration,
) -> Result<(), RestrictError> {
    let mut edges: Vec<&Edge> = src.edges.keys().collect();
    edges.sort_unstable();
    for edge in edges {
        if let Some(image) = carried_edge(migration, edge) {
            if !tgt.edges.contains_key(image) {
                return Err(RestrictError::NoEdgeFound {
                    src: image.src.to_string(),
                    tgt: image.tgt.to_string(),
                });
            }
        }
    }
    Ok(())
}

/// The internal arrows of a fibre: the edges of the source schema whose
/// endpoints both lie in the fibre and which the migration carries to no
/// target edge, in ascending order.
fn internal_arrows<'a>(
    schema: &'a Schema,
    migration: &CompiledMigration,
    fiber: &[Name],
) -> Vec<&'a Edge> {
    let mut arrows: Vec<&Edge> = fiber
        .iter()
        .flat_map(|src| fiber.iter().map(move |tgt| (src, tgt)))
        .flat_map(|(src, tgt)| schema.edges_between(src, tgt))
        .filter(|edge| carried_edge(migration, edge).is_none())
        .collect();
    arrows.sort_unstable();
    arrows.dedup();
    arrows
}

/// Build the rows of one target vertex's limit, each with the source row it
/// drew from every fibre component.
///
/// Every vertex in the fibre is a factor of the product. The empty fibre is
/// the empty product: one row, drawing from no component. A factor whose
/// table is empty or absent empties the whole product: the limit contains no
/// tuple that could supply that component. Such a factor is never dropped
/// from the fibre, which would return the product over the remaining factors
/// (rows the limit does not contain). A product row survives only when its
/// components form a pair of every internal arrow.
///
/// # Errors
///
/// Returns [`RestrictError::ProductSizeExceeded`] if the Cartesian product
/// would exceed `max_product_size`, and [`RestrictError::AttributeCollision`]
/// when two components disagree on a shared column.
fn build_fiber_table(
    instance: &FInstance,
    src_vertices: &[Name],
    tgt_vertex: &Name,
    internal: &[&Edge],
    max_product_size: usize,
) -> Result<FiberRows, RestrictError> {
    if src_vertices.is_empty() {
        return Ok(vec![(Row::new(), BTreeMap::new())]);
    }

    let mut factors: Vec<(&Name, Vec<(RowId, &Row)>)> = Vec::with_capacity(src_vertices.len());
    for src_v in src_vertices {
        let rows = instance.rows(src_v);
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        factors.push((src_v, rows));
    }

    if factors.len() > 1 {
        let product_size = factors
            .iter()
            .try_fold(1_usize, |acc, (_, rows)| acc.checked_mul(rows.len()))
            .unwrap_or(usize::MAX);
        if product_size > max_product_size {
            return Err(RestrictError::ProductSizeExceeded {
                vertex: tgt_vertex.to_string(),
                actual: product_size,
                limit: max_product_size,
            });
        }
    }

    let mut product: FiberRows = vec![(Row::new(), BTreeMap::new())];
    for (src_v, rows) in &factors {
        let mut next = Vec::with_capacity(product.len() * rows.len());
        for (existing, components) in &product {
            for (id, row) in rows {
                let mut merged = existing.clone();
                for (col, val) in *row {
                    // Attribute discipline: a column already contributed by an
                    // earlier fibre component must agree, or the product-row
                    // merge would silently overwrite one value with another.
                    if let Some(prior) = merged.get(col) {
                        if prior != val {
                            return Err(RestrictError::AttributeCollision {
                                vertex: tgt_vertex.to_string(),
                                column: col.clone(),
                            });
                        }
                    }
                    merged.insert(col.clone(), val.clone());
                }
                let mut drawn = components.clone();
                drawn.insert((*src_v).clone(), *id);
                next.push((merged, drawn));
            }
        }
        product = next;
    }

    product.retain(|(_, components)| {
        internal.iter().all(
            |edge| match (components.get(&edge.src), components.get(&edge.tgt)) {
                (Some(&s), Some(&t)) => instance.contains_pair(edge, s, t),
                _ => false,
            },
        )
    });
    Ok(product)
}

/// Build the fiber map from a migration's vertex remap and surviving vertices.
///
/// Target vertices come in ascending name and each fiber's source vertices
/// in name order, so downstream
/// consumers see a factor order that does not depend on hash iteration order.
fn build_fiber_map(migration: &CompiledMigration) -> BTreeMap<Name, Vec<Name>> {
    let mut fiber_map: BTreeMap<Name, Vec<Name>> = BTreeMap::new();
    let remap_targets: std::collections::HashSet<&Name> = migration.vertex_remap.values().collect();

    for (src, tgt) in &migration.vertex_remap {
        fiber_map.entry(tgt.clone()).or_default().push(src.clone());
    }

    for sv in &migration.surviving_verts {
        if !migration.vertex_remap.contains_key(sv) && !remap_targets.contains(sv) {
            fiber_map.entry(sv.clone()).or_default().push(sv.clone());
        }
    }

    for sources in fiber_map.values_mut() {
        sources.sort_unstable();
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
            return Err(RestrictError::NonInjectiveVertexMap {
                target: tgt.clone(),
                sources: srcs.clone(),
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
    use std::collections::{BTreeSet, HashSet};

    use panproto_schema::Vertex;
    use smallvec::smallvec;

    use super::*;
    use crate::value::Value;

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
            edges: edges.iter().map(|e| (e.clone(), e.kind.clone())).collect(),
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
            ..Default::default()
        }
    }

    /// The source and target schemas a table test runs Pi against: the
    /// source declares every foreign key of the instance, the `declared`
    /// edges and the edges the migration carries; the target declares their
    /// images.
    fn schemas(
        inst: &FInstance,
        migration: &CompiledMigration,
        declared: &[Edge],
    ) -> (Schema, Schema) {
        let mut src: Vec<Edge> = inst
            .foreign_keys()
            .into_iter()
            .map(|(e, _)| e.clone())
            .collect();
        src.extend(declared.iter().cloned());
        src.extend(migration.edge_remap.keys().cloned());
        src.extend(migration.surviving_edges.iter().cloned());
        src.sort_unstable();
        src.dedup();
        let mut tgt: Vec<Edge> = migration.edge_remap.values().cloned().collect();
        tgt.extend(migration.surviving_edges.iter().cloned());
        tgt.sort_unstable();
        tgt.dedup();
        (make_test_schema(&[], &src), make_test_schema(&[], &tgt))
    }

    /// Pi along `migration`, against the schemas of [`schemas`].
    fn run_pi(
        inst: &FInstance,
        migration: &CompiledMigration,
        declared: &[Edge],
        max: usize,
    ) -> Result<(FInstance, BTreeMap<Name, FInstanceHom>), RestrictError> {
        let (src, tgt) = schemas(inst, migration, declared);
        functor_pi_with_hom(inst, &src, &tgt, migration, max)
    }

    // --- functor_pi tests ---

    #[test]
    fn functor_pi_single_fiber_copies_table() {
        let mut row = HashMap::new();
        row.insert("name".to_string(), Value::Str("Alice".into()));
        let inst = FInstance::new()
            .with_table("users", vec![row.clone()])
            .unwrap();

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

        let result = run_pi(&inst, &migration, &[], 100)
            .map(|(pi, _)| pi)
            .unwrap();
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
            .and_then(|f| f.with_table("src_b", rows_b))
            .unwrap();

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

        let result = run_pi(&inst, &migration, &[], 100)
            .map(|(pi, _)| pi)
            .unwrap();
        // 2 * 1 = 2 product rows
        assert_eq!(result.row_count("merged"), 2);
        // Each product row should have both x and y columns
        for (_, row) in result.rows("merged") {
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
            .and_then(|f| f.with_table("src_b", rows_b))
            .unwrap();

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
        let result = run_pi(&inst, &migration, &[], 2).map(|(pi, _)| pi);
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
        // src_a holds rows 0 and 1, src_c row 2, other rows 3 and 4; src_a
        // row 1 references other row 3.
        let inst = FInstance::new()
            .with_table("src_a", rows_a)
            .and_then(|f| f.with_table("src_c", rows_c))
            .and_then(|f| f.with_table("other", rows_o))
            .and_then(|f| f.with_foreign_key(fk_edge.clone(), [(RowId(1), RowId(3))]))
            .unwrap();

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

        let result = run_pi(&inst, &migration, &[], 100)
            .map(|(pi, _)| pi)
            .unwrap();
        assert_eq!(result.row_count("merged"), 2);
        assert_eq!(result.row_count("other"), 2);

        // The product rows are fresh, from 5 up: (a0, c2) is 5 and (a1, c2)
        // is 6. `other` is a one-component fibre and keeps its ids.
        assert_eq!(
            result.pairs(&new_edge),
            &BTreeSet::from([(RowId(6), RowId(3))])
        );
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
        // src_a holds rows 0 and 1, src_a2 row 2, src_b rows 3 and 4, src_b2
        // row 5; src_a row 0 references src_b row 4.
        let inst = FInstance::new()
            .with_table("src_a", rows_a)
            .and_then(|f| f.with_table("src_a2", rows_p))
            .and_then(|f| f.with_table("src_b", rows_b))
            .and_then(|f| f.with_table("src_b2", rows_q))
            .and_then(|f| f.with_foreign_key(fk_edge.clone(), [(RowId(0), RowId(4))]))
            .unwrap();

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

        let result = run_pi(&inst, &migration, &[], 100)
            .map(|(pi, _)| pi)
            .unwrap();
        assert_eq!(result.row_count("m1"), 2);
        assert_eq!(result.row_count("m2"), 2);

        // m1's product rows are 6 = (a0, a2_2) and 7 = (a1, a2_2); m2's are
        // 8 = (b3, b2_5) and 9 = (b4, b2_5).
        assert_eq!(
            result.pairs(&new_edge),
            &BTreeSet::from([(RowId(6), RowId(9))])
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

    /// A fiber component with an empty table is a factor of the product, so it
    /// empties the product rather than being dropped from the fiber.
    #[test]
    fn functor_pi_empty_factor_empties_product() {
        let rows_a = vec![
            HashMap::from([("x".to_string(), Value::Int(1))]),
            HashMap::from([("x".to_string(), Value::Int(2))]),
        ];
        let inst = FInstance::new()
            .with_table("src_a", rows_a)
            .and_then(|f| f.with_table("src_b", Vec::new()))
            .unwrap();

        let mut vertex_remap = HashMap::new();
        vertex_remap.insert(Name::from("src_a"), Name::from("merged"));
        vertex_remap.insert(Name::from("src_b"), Name::from("merged"));

        let mut migration = CompiledMigration::default();
        migration.surviving_verts.insert(Name::from("merged"));
        migration.vertex_remap = vertex_remap;

        let result = run_pi(&inst, &migration, &[], 100)
            .map(|(pi, _)| pi)
            .unwrap();
        assert_eq!(
            result.row_count("merged"),
            0,
            "a product with an empty factor has no rows",
        );
    }

    /// An absent source table is the empty table, so it empties the product the
    /// same way a present-but-empty one does.
    #[test]
    fn functor_pi_absent_factor_empties_product() {
        let rows_a = vec![HashMap::from([("x".to_string(), Value::Int(1))])];
        let inst = FInstance::new().with_table("src_a", rows_a).unwrap();

        let mut vertex_remap = HashMap::new();
        vertex_remap.insert(Name::from("src_a"), Name::from("merged"));
        vertex_remap.insert(Name::from("src_b"), Name::from("merged"));

        let mut migration = CompiledMigration::default();
        migration.surviving_verts.insert(Name::from("merged"));
        migration.vertex_remap = vertex_remap;

        let result = run_pi(&inst, &migration, &[], 100)
            .map(|(pi, _)| pi)
            .unwrap();
        assert_eq!(
            result.row_count("merged"),
            0,
            "an absent factor is the empty table, so the product is empty",
        );
    }

    /// Distinct source edges identified by the edge map contribute a union of
    /// pairs, and the result does not depend on hash iteration order.
    #[test]
    fn functor_pi_unions_collided_edges_deterministically() {
        fn self_edge(v: &str) -> Edge {
            Edge {
                src: v.into(),
                tgt: v.into(),
                kind: "ref".into(),
                name: Some("link".into()),
            }
        }

        let build = || {
            let inst = FInstance::new()
                .with_table(
                    "a",
                    vec![
                        HashMap::from([("n".to_string(), Value::Int(0))]),
                        HashMap::from([("n".to_string(), Value::Int(1))]),
                    ],
                )
                .and_then(|f| {
                    f.with_table(
                        "b",
                        vec![
                            HashMap::from([("m".to_string(), Value::Int(0))]),
                            HashMap::from([("m".to_string(), Value::Int(1))]),
                        ],
                    )
                })
                .and_then(|f| f.with_foreign_key(self_edge("a"), [(RowId(0), RowId(1))]))
                .and_then(|f| f.with_foreign_key(self_edge("b"), [(RowId(3), RowId(2))]))
                .unwrap();

            let mut migration = CompiledMigration::default();
            migration.surviving_verts.insert(Name::from("t"));
            migration
                .vertex_remap
                .insert(Name::from("a"), Name::from("t"));
            migration
                .vertex_remap
                .insert(Name::from("b"), Name::from("t"));
            migration.edge_remap.insert(self_edge("a"), self_edge("t"));
            migration.edge_remap.insert(self_edge("b"), self_edge("t"));
            run_pi(&inst, &migration, &[], 100)
                .map(|(pi, _)| pi)
                .unwrap()
        };

        let first = build();
        let pairs = first.pairs(&self_edge("t")).clone();
        // `a` holds rows 0 and 1, `b` rows 2 and 3. Fiber components are
        // ordered by name, so the product rows are 4 = (a0, b2), 5 = (a0, b3),
        // 6 = (a1, b2), 7 = (a1, b3). The `a` edge (0, 1) fans out to every
        // product row drawing a0 crossed with every row drawing a1; the `b`
        // edge (3, 2) fans out the same way over b.
        let r = |a: u32, b: u32| (RowId(a), RowId(b));
        assert_eq!(
            pairs,
            BTreeSet::from([
                r(4, 6),
                r(4, 7),
                r(5, 4),
                r(5, 6),
                r(5, 7),
                r(7, 4),
                r(7, 6)
            ]),
            "both source edges contribute their fanned-out product pairs",
        );

        for _ in 0..16 {
            let again = build();
            assert_eq!(
                again.pairs(&self_edge("t")),
                first.pairs(&self_edge("t")),
                "merged foreign keys must not depend on hash iteration order",
            );
        }
    }

    /// Two two-row tables merged into one target vertex, with a foreign key
    /// from `src_a` to `src_b` that the migration carries nowhere.
    fn intra_fibre_fixture() -> (FInstance, CompiledMigration, Edge) {
        let fk = Edge {
            src: "src_a".into(),
            tgt: "src_b".into(),
            kind: "fk".into(),
            name: Some("ref".into()),
        };
        let cell = |k: &str, v: i64| HashMap::from([(k.to_owned(), Value::Int(v))]);
        // src_a holds rows 0 (a0) and 1 (a1), src_b rows 2 (b0) and 3 (b1);
        // the foreign key relates a0 to b1.
        let inst = FInstance::new()
            .with_table("src_a", vec![cell("a", 0), cell("a", 1)])
            .and_then(|f| f.with_table("src_b", vec![cell("b", 0), cell("b", 1)]))
            .and_then(|f| f.with_foreign_key(fk.clone(), [(RowId(0), RowId(3))]))
            .unwrap();
        let mut migration = CompiledMigration::default();
        migration.surviving_verts.insert(Name::from("m"));
        migration
            .vertex_remap
            .insert(Name::from("src_a"), Name::from("m"));
        migration
            .vertex_remap
            .insert(Name::from("src_b"), Name::from("m"));
        (inst, migration, fk)
    }

    #[test]
    fn functor_pi_equalises_along_an_intra_fibre_foreign_key() {
        let (inst, migration, _) = intra_fibre_fixture();
        let result = run_pi(&inst, &migration, &[], 100)
            .map(|(pi, _)| pi)
            .unwrap();
        let rows: Vec<(i64, i64)> = result
            .rows("m")
            .into_iter()
            .map(|(_, row)| {
                let cell_of = |k: &str| match row.get(k) {
                    Some(Value::Int(v)) => *v,
                    other => panic!("cell {k} is {other:?}"),
                };
                (cell_of("a"), cell_of("b"))
            })
            .collect();
        assert_eq!(
            rows,
            vec![(0, 1)],
            "the limit keeps only the product rows whose components the internal arrow relates",
        );
    }

    /// An internal arrow with no pairs is still an arrow of the fibre: the
    /// limit equalises along the empty relation and is empty. Adding a pair
    /// can only enlarge the limit.
    #[test]
    fn an_internal_foreign_key_without_pairs_empties_the_limit() {
        let (with_pair, migration, fk) = intra_fibre_fixture();
        let mut without_pair = with_pair.clone();
        assert!(without_pair.remove_foreign_key(&fk, RowId(0), RowId(3)));
        let with_rows = run_pi(&with_pair, &migration, &[], 100)
            .map(|(pi, _)| pi)
            .unwrap()
            .row_count("m");
        let without_rows = run_pi(&without_pair, &migration, std::slice::from_ref(&fk), 100)
            .map(|(pi, _)| pi)
            .unwrap()
            .row_count("m");
        assert_eq!((with_rows, without_rows), (1, 0));
    }

    #[test]
    fn a_carried_edge_the_target_lacks_is_refused() {
        let (inst, mut migration, fk) = intra_fibre_fixture();
        let loop_edge = Edge {
            src: "m".into(),
            tgt: "m".into(),
            kind: "fk".into(),
            name: Some("ref".into()),
        };
        migration.edge_remap.insert(fk, loop_edge);
        let (src, _) = schemas(&inst, &migration, &[]);
        assert!(matches!(
            functor_pi(&inst, &src, &Schema::default(), &migration, 100),
            Err(RestrictError::NoEdgeFound { .. })
        ));
    }

    #[test]
    fn a_carried_intra_fibre_foreign_key_fans_out_over_the_product() {
        let (inst, mut migration, fk) = intra_fibre_fixture();
        let loop_edge = Edge {
            src: "m".into(),
            tgt: "m".into(),
            kind: "fk".into(),
            name: Some("ref".into()),
        };
        migration.edge_remap.insert(fk, loop_edge.clone());
        let result = run_pi(&inst, &migration, &[], 100)
            .map(|(pi, _)| pi)
            .unwrap();
        // Not an internal arrow: the product is not equalised, and the pair
        // (a0, b1) relates every row drawing a0 to every row drawing b1.
        assert_eq!(result.row_count("m"), 4);
        let r = |a: u32, b: u32| (RowId(a), RowId(b));
        assert_eq!(
            result.pairs(&loop_edge),
            &BTreeSet::from([r(4, 5), r(4, 7), r(5, 5), r(5, 7)])
        );
    }

    #[test]
    fn functor_pi_allocates_rows_in_product_order() {
        let (inst, migration, _) = intra_fibre_fixture();
        let mut unconstrained = inst.clone();
        let fk = unconstrained.all_pairs()[0].clone();
        assert!(unconstrained.remove_foreign_key(&fk.0, fk.1, fk.2));
        let (result, projections) = run_pi(&unconstrained, &migration, &[], 100).unwrap();
        // Fresh ids start above the input's largest (3), in product order:
        // src_a varies slowest, src_b fastest.
        assert_eq!(
            result.table("m").unwrap().ids(),
            vec![RowId(4), RowId(5), RowId(6), RowId(7)]
        );
        let onto = |src: &str| {
            projections[&Name::from(src)]
                .row_map
                .values()
                .copied()
                .collect::<Vec<_>>()
        };
        assert_eq!(onto("src_a"), vec![RowId(0), RowId(0), RowId(1), RowId(1)]);
        assert_eq!(onto("src_b"), vec![RowId(2), RowId(3), RowId(2), RowId(3)]);
        // Each projection preserves cells column by column.
        for (product_row, component) in &projections[&Name::from("src_b")].row_map {
            assert_eq!(
                result.row(*product_row).unwrap().get("b"),
                inst.row(*component).unwrap().get("b")
            );
        }
    }

    #[test]
    fn a_one_component_fibre_keeps_its_row_ids() {
        let inst = FInstance::new()
            .with_rows(
                "users",
                [(RowId(41), HashMap::new()), (RowId(7), HashMap::new())],
            )
            .unwrap();
        let mut migration = CompiledMigration::default();
        migration.surviving_verts.insert(Name::from("users"));
        let (result, projections) = run_pi(&inst, &migration, &[], 10).unwrap();
        assert_eq!(
            result.table("users").unwrap().ids(),
            vec![RowId(7), RowId(41)]
        );
        assert_eq!(
            projections[&Name::from("users")].row_map,
            BTreeMap::from([(RowId(7), RowId(7)), (RowId(41), RowId(41))])
        );
    }

    /// Carrying foreign keys through Pi is linear in pairs plus rows: each
    /// pair reads the result rows drawn from its two source rows through an
    /// index, not by scanning the endpoint tables.
    #[test]
    fn carrying_foreign_keys_through_pi_is_linear() {
        const ROWS: u32 = 20_000;
        let edge = Edge {
            src: "a".into(),
            tgt: "b".into(),
            kind: "fk".into(),
            name: None,
        };
        let cells = |n: u32| HashMap::from([("n".to_owned(), Value::Int(i64::from(n)))]);
        let inst = FInstance::new()
            .with_table("a", (0..ROWS).map(cells))
            .and_then(|f| f.with_table("b", (0..ROWS).map(cells)))
            .and_then(|f| {
                f.with_foreign_key(edge.clone(), (0..ROWS).map(|i| (RowId(i), RowId(ROWS + i))))
            })
            .unwrap();
        let migration = CompiledMigration {
            surviving_verts: HashSet::from([Name::from("a"), Name::from("b")]),
            surviving_edges: HashSet::from([edge]),
            ..CompiledMigration::default()
        };
        let began = std::time::Instant::now();
        let (pi, _) = run_pi(&inst, &migration, &[], usize::MAX).unwrap();
        let took = began.elapsed();
        assert_eq!(pi, inst, "Pi along the identity is the identity");
        assert!(
            took < std::time::Duration::from_secs(3),
            "Pi over {ROWS} pairs took {took:?}"
        );
    }
}
