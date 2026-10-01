//! Right Kan extension (`Pi_F`) for instances.
//!
//! The right Kan extension computes the "limit" (product) over fibers
//! of a migration morphism. For set-valued functor instances this means
//! taking Cartesian products of rows when multiple source vertices map
//! to the same target vertex. For W-type instances, only vertex-injective
//! migrations are supported; a non-injective map is rejected rather than
//! silently producing `Sigma`-shaped output mislabeled as `Pi`.

use std::collections::{BTreeMap, HashMap, HashSet};

use panproto_gat::Name;
use panproto_schema::{Edge, Schema};

use crate::error::RestrictError;
use crate::functor::{FInstance, Row, allocate_row, row_allocator, same_value};
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
/// The result has a table at each vertex in the image of `F`, the images of
/// the migration's source vertices (read as every table operation reads
/// them; see [`crate::functor_restrict`]).
/// For each such target vertex, the fibre is the set of source vertices that
/// map to it, and the target's table is the limit over the fibre: the
/// Cartesian product of the component tables with column union, equalised
/// along every internal arrow. An internal arrow is an edge of `src_schema`
/// whose endpoints both lie in the fibre and which is no source edge of the
/// migration, so `F` sends it to the
/// identity of the target vertex; a product row is kept only when its
/// components are a pair of that foreign key. The arrows are read from the
/// schema, not from the instance, so an internal foreign key without pairs
/// is the empty relation and empties the limit. A source edge of the
/// migration with both endpoints in the fibre goes to a loop on the target
/// vertex, not to its identity, and then the limit ranges over every iterate
/// of that loop; the fibre's limit is that limit only for a single vertex
/// with no internal arrow whose carried loops go to distinct loops, and every
/// other fibre holding such an edge is refused rather than returning the
/// unequalised product. A fibre component whose
/// table is empty or absent is an empty factor, so the target's table is
/// empty. A vertex of `tgt_schema` outside the image of `F` has an empty
/// fibre and gets no table: the schema does not say which of its vertices
/// hold tables (a column vertex is an attribute, not a table), so the result
/// is computed over the image of `F` alone.
///
/// Row ids: a fibre with one component and no internal arrow keeps its rows'
/// ids (the limit is the component itself); every other row of the result is
/// fresh, allocated from the least id above every row of the input, the
/// target vertices taken in ascending name and each target's rows in the
/// product's order (components in ascending name, the last varying fastest).
///
/// Foreign keys travel along the source edges, each to its image, as every
/// table operation reads them; the pairs of any other edge are not carried.
/// A target edge relates result rows `p` and `q` exactly when every source
/// edge `e` carried to it relates the row `p` drew from `e`'s source to the
/// row `q` drew from `e`'s target. With one such edge this relates every row
/// drawing `i` to every row drawing `j` for each pair `(i, j)`; edges the
/// migration merges constrain each its own components, so `Pi` of the merged
/// edge is their intersection, and a carried edge without pairs empties it.
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
/// components of one product row disagree on a shared column,
/// [`RestrictError::CarriedFibreArrow`] when a fibre holds a source edge of
/// the migration as described above, and [`RestrictError::RowIdsExhausted`]
/// when the fresh row ids run out.
pub fn functor_pi_with_hom(
    instance: &FInstance,
    src_schema: &Schema,
    tgt_schema: &Schema,
    migration: &CompiledMigration,
    max_product_size: usize,
) -> Result<(FInstance, BTreeMap<Name, FInstanceHom>), RestrictError> {
    let edge_images = crate::adjunction::edge_images(migration);
    check_carried_edges(src_schema, tgt_schema, &edge_images)?;
    let fiber_map = build_fiber_map(migration);
    let mut allocator = row_allocator(instance.row_ids());

    let mut out = FInstance::new();
    let mut projections: BTreeMap<Name, FInstanceHom> = BTreeMap::new();
    let mut provenance: HashMap<Name, PlacedRows> = HashMap::new();

    for (tgt_vertex, src_vertices) in &fiber_map {
        let internal = internal_arrows(src_schema, &edge_images, src_vertices);
        check_carried_fibre_arrows(tgt_vertex, src_vertices, &edge_images, &internal)?;
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

    carry_foreign_keys(instance, &edge_images, &fibre_of, &provenance, &mut out);

    Ok((out, projections))
}

/// Carry the foreign keys of `instance` into `out`, the result of `Pi_F`, as
/// [`functor_pi_with_hom`] states: a target edge relates result rows `p` and
/// `q` exactly when every source edge carried to it relates the rows `p` and
/// `q` drew from its endpoints. `fibre_of` sends each source vertex to its
/// target vertex and `provenance` lists, per target table, each placed row
/// with the source row it drew from every fibre component.
fn carry_foreign_keys(
    instance: &FInstance,
    edge_images: &HashMap<Edge, Edge>,
    fibre_of: &HashMap<&Name, &Name>,
    provenance: &HashMap<Name, PlacedRows>,
    out: &mut FInstance,
) {
    // The result rows drawn from each source row, per target table and
    // fibre component, so carrying a pair reads only the rows it relates,
    // and the components each result row drew.
    let mut drawn_from: HashMap<(&Name, &Name, RowId), Vec<RowId>> = HashMap::new();
    let mut components_of: HashMap<RowId, &BTreeMap<Name, RowId>> = HashMap::new();
    for (tgt_vertex, placed) in provenance {
        for (id, components) in placed {
            components_of.insert(*id, components);
            for (component, &src_row) in components {
                drawn_from
                    .entry((tgt_vertex, component, src_row))
                    .or_default()
                    .push(*id);
            }
        }
    }
    // Each target edge with the source edges carried to it whose endpoints
    // lie in the fibres of its endpoints, in ascending order.
    let mut preimages: BTreeMap<&Edge, Vec<&Edge>> = BTreeMap::new();
    for (edge, image) in edge_images {
        if fibre_of.get(&edge.src) == Some(&&image.src)
            && fibre_of.get(&edge.tgt) == Some(&&image.tgt)
        {
            preimages.entry(image).or_default().push(edge);
        }
    }
    for (image, mut edges) in preimages {
        edges.sort_unstable();
        let Some((&first, rest)) = edges.split_first() else {
            continue;
        };
        // A pair of the result relates `p` to `q` exactly when every carried
        // edge relates the components they drew from its endpoints.
        let related = |p: RowId, q: RowId| {
            rest.iter().all(|edge| {
                match (
                    components_of.get(&p).and_then(|c| c.get(&edge.src)),
                    components_of.get(&q).and_then(|c| c.get(&edge.tgt)),
                ) {
                    (Some(&i), Some(&j)) => instance.contains_pair(edge, i, j),
                    _ => false,
                }
            })
        };
        for &(i, j) in instance.pairs(first) {
            let (Some(sources), Some(targets)) = (
                drawn_from.get(&(&image.src, &first.src, i)),
                drawn_from.get(&(&image.tgt, &first.tgt, j)),
            ) else {
                continue;
            };
            for &p in sources {
                for &q in targets {
                    if related(p, q) {
                        out.place_pair(image.clone(), p, q);
                    }
                }
            }
        }
    }
}

/// Check that every edge of `src` the migration carries lands on an edge of
/// `tgt`, taking the edges in ascending order. The carried edges and their
/// images are `edge_images`, read by [`crate::adjunction::edge_images`].
fn check_carried_edges(
    src: &Schema,
    tgt: &Schema,
    edge_images: &HashMap<Edge, Edge>,
) -> Result<(), RestrictError> {
    let mut edges: Vec<&Edge> = src.edges.keys().collect();
    edges.sort_unstable();
    for edge in edges {
        if let Some(image) = edge_images.get(edge) {
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
    edge_images: &HashMap<Edge, Edge>,
    fiber: &[Name],
) -> Vec<&'a Edge> {
    let mut arrows: Vec<&Edge> = fiber
        .iter()
        .flat_map(|src| fiber.iter().map(move |tgt| (src, tgt)))
        .flat_map(|(src, tgt)| schema.edges_between(src, tgt))
        .filter(|edge| !edge_images.contains_key(*edge))
        .collect();
    arrows.sort_unstable();
    arrows.dedup();
    arrows
}

/// Refuse a fibre whose limit is not `Pi_F` at its target vertex: one holding
/// an edge, both endpoints in the fibre, that the migration carries to an
/// edge rather than to the identity of the target vertex.
///
/// Such an edge goes to a loop `l` on the vertex, so the comma category the
/// right Kan extension takes its limit over holds an object for every
/// iterate of `l`. The fibre's limit is that limit only when each iterate is
/// reached from the fibre along exactly one path: the fibre is a single
/// vertex with no edge sent to the identity, and its carried loops go to
/// distinct loops on the target vertex. Any other fibre holding a carried
/// edge is refused, naming the least such edge in a fibre of several
/// vertices or beside an edge sent to the identity, and otherwise the
/// first loop, in ascending order, whose image an earlier loop already has.
///
/// # Errors
///
/// Returns [`RestrictError::CarriedFibreArrow`] as described above.
fn check_carried_fibre_arrows(
    vertex: &Name,
    fiber: &[Name],
    edge_images: &HashMap<Edge, Edge>,
    internal: &[&Edge],
) -> Result<(), RestrictError> {
    let mut carried: Vec<(&Edge, &Edge)> = edge_images
        .iter()
        .filter(|(edge, _)| fiber.contains(&edge.src) && fiber.contains(&edge.tgt))
        .collect();
    carried.sort_unstable();
    let refuse = |edge: &Edge| RestrictError::CarriedFibreArrow {
        vertex: vertex.clone(),
        edge: edge.clone(),
    };
    if let Some(&(first, _)) = carried.first() {
        if fiber.len() > 1 || !internal.is_empty() {
            return Err(refuse(first));
        }
    }
    let mut images: HashSet<&Edge> = HashSet::with_capacity(carried.len());
    for (edge, image) in carried {
        let is_loop = image.src == *vertex && image.tgt == *vertex;
        if !is_loop || !images.insert(image) {
            return Err(refuse(edge));
        }
    }
    Ok(())
}

/// Build the rows of one target vertex's limit, each with the source row it
/// drew from every fibre component.
///
/// Every vertex in the fibre is a factor of the product; the fibre is never
/// empty, since it is read off the source vertices. A factor whose
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
                    // earlier fibre component must agree, cell for cell by
                    // canonical bytes, or the product-row merge would silently
                    // overwrite one value with another.
                    if let Some(prior) = merged.get(col) {
                        if !same_value(prior, val) {
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

/// Build the fiber map of a migration: each target vertex with the source
/// vertices it is the image of.
///
/// The source vertices are the migration's domain as
/// [`crate::adjunction::source_vertices`] reads it, the one reading every
/// table operation shares. Target vertices come in ascending name and each
/// fiber's source vertices in name order, so downstream consumers see a
/// factor order that does not depend on hash iteration order.
fn build_fiber_map(migration: &CompiledMigration) -> BTreeMap<Name, Vec<Name>> {
    let mut fiber_map: BTreeMap<Name, Vec<Name>> = BTreeMap::new();
    for source in crate::adjunction::source_vertices(migration) {
        let target = crate::adjunction::map_vertex(migration, &source);
        fiber_map.entry(target).or_default().push(source);
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

    /// A loop on each vertex of a two-vertex fibre, both carried to one loop
    /// on the target: the limit ranges over every iterate of that loop, so
    /// Pi refuses, naming the least carried edge of the fibre.
    #[test]
    fn carried_loops_in_a_two_vertex_fibre_are_refused() {
        fn self_edge(v: &str) -> Edge {
            Edge {
                src: v.into(),
                tgt: v.into(),
                kind: "ref".into(),
                name: Some("link".into()),
            }
        }

        let inst = FInstance::new()
            .with_table("a", vec![HashMap::from([("n".to_string(), Value::Int(0))])])
            .and_then(|f| {
                f.with_table("b", vec![HashMap::from([("m".to_string(), Value::Int(0))])])
            })
            .and_then(|f| f.with_foreign_key(self_edge("a"), [(RowId(0), RowId(0))]))
            .and_then(|f| f.with_foreign_key(self_edge("b"), [(RowId(1), RowId(1))]))
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
        assert_carried_fibre_arrow(run_pi(&inst, &migration, &[], 100), "t", &self_edge("a"));
    }

    /// Assert that Pi refused with [`RestrictError::CarriedFibreArrow`] at
    /// `vertex`, naming `edge`.
    fn assert_carried_fibre_arrow<T: std::fmt::Debug>(
        result: Result<T, RestrictError>,
        vertex: &str,
        edge: &Edge,
    ) {
        match result {
            Err(RestrictError::CarriedFibreArrow { vertex: v, edge: e }) => {
                assert_eq!((v.as_ref(), &e), (vertex, edge));
            }
            other => panic!("expected CarriedFibreArrow at {vertex} on {edge:?}, got {other:?}"),
        }
    }

    /// A loop edge `v -> v` of kind `fk` named `name`.
    fn loop_on(v: &str, name: &str) -> Edge {
        Edge {
            src: v.into(),
            tgt: v.into(),
            kind: "fk".into(),
            name: Some(name.into()),
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

    /// The intra-fibre foreign key remapped to a loop on `m`: Pi along a
    /// functor that sends an arrow of the fibre to a loop is a limit over
    /// every iterate of the loop, not the product, so it refuses.
    #[test]
    fn pi_dispatch_rejects_a_fibre_with_an_internal_edge() {
        let (inst, mut migration, fk) = intra_fibre_fixture();
        migration.edge_remap.insert(fk.clone(), loop_on("m", "ref"));
        assert_carried_fibre_arrow(run_pi(&inst, &migration, &[], 100), "m", &fk);
    }

    /// The same foreign key listed as surviving: the migration relabels it to
    /// the loop on `m`, so Pi refuses it as it refuses the remapped one.
    #[test]
    fn a_surviving_intra_fibre_edge_is_refused() {
        let (inst, mut migration, fk) = intra_fibre_fixture();
        migration.surviving_edges.insert(fk.clone());
        let src = make_test_schema(&[], std::slice::from_ref(&fk));
        let tgt = make_test_schema(&[], &[loop_on("m", "ref")]);
        assert_carried_fibre_arrow(functor_pi(&inst, &src, &tgt, &migration, 100), "m", &fk);
    }

    /// A one-vertex fibre `{a}` whose loop is relabelled onto `t`: every
    /// object of the comma category is reached from `a` along exactly one
    /// path, so the limit is `a`'s table, its rows keep their ids and the
    /// loop travels with its pairs.
    #[test]
    fn a_one_vertex_fibre_carries_its_loop() {
        let cell = |v: i64| HashMap::from([("n".to_owned(), Value::Int(v))]);
        let inst = FInstance::new()
            .with_table("a", vec![cell(0), cell(1)])
            .and_then(|f| f.with_foreign_key(loop_on("a", "next"), [(RowId(0), RowId(1))]))
            .unwrap();
        let mut migration = CompiledMigration::default();
        migration.surviving_verts.insert(Name::from("t"));
        migration
            .vertex_remap
            .insert(Name::from("a"), Name::from("t"));
        migration
            .edge_remap
            .insert(loop_on("a", "next"), loop_on("t", "next"));
        let (pi, _) = run_pi(&inst, &migration, &[], 100).unwrap();
        let ids: Vec<RowId> = pi.rows("t").into_iter().map(|(id, _)| id).collect();
        assert_eq!(ids, vec![RowId(0), RowId(1)]);
        assert_eq!(
            pi.pairs(&loop_on("t", "next")),
            &BTreeSet::from([(RowId(0), RowId(1))])
        );
    }

    /// Two loops of a one-vertex fibre carried to one loop: a path of length
    /// `n` reaches the `n`-th iterate in `2^n` ways, so the limit equalises
    /// the two loops on every iterate and Pi refuses, naming the second loop.
    #[test]
    fn a_one_vertex_fibre_whose_loops_share_an_image_is_refused() {
        let inst = FInstance::new()
            .with_table("a", vec![HashMap::new()])
            .and_then(|f| f.with_foreign_key(loop_on("a", "p"), [(RowId(0), RowId(0))]))
            .and_then(|f| f.with_foreign_key(loop_on("a", "q"), [(RowId(0), RowId(0))]))
            .unwrap();
        let mut migration = CompiledMigration::default();
        migration.surviving_verts.insert(Name::from("t"));
        migration
            .vertex_remap
            .insert(Name::from("a"), Name::from("t"));
        migration
            .edge_remap
            .insert(loop_on("a", "p"), loop_on("t", "p"));
        migration
            .edge_remap
            .insert(loop_on("a", "q"), loop_on("t", "p"));
        assert_carried_fibre_arrow(run_pi(&inst, &migration, &[], 100), "t", &loop_on("a", "q"));
    }

    /// A one-vertex fibre with one loop sent to the identity and one carried
    /// to a loop: the factorisations of the carried loop through the fibre
    /// are not connected, so the limit is not the equalised table and Pi
    /// refuses.
    #[test]
    fn a_carried_loop_beside_a_loop_sent_to_the_identity_is_refused() {
        let inst = FInstance::new()
            .with_table("a", vec![HashMap::new()])
            .and_then(|f| f.with_foreign_key(loop_on("a", "c"), [(RowId(0), RowId(0))]))
            .and_then(|f| f.with_foreign_key(loop_on("a", "i"), [(RowId(0), RowId(0))]))
            .unwrap();
        let mut migration = CompiledMigration::default();
        migration.surviving_verts.insert(Name::from("t"));
        migration
            .vertex_remap
            .insert(Name::from("a"), Name::from("t"));
        migration
            .edge_remap
            .insert(loop_on("a", "c"), loop_on("t", "c"));
        assert_carried_fibre_arrow(run_pi(&inst, &migration, &[], 100), "t", &loop_on("a", "c"));
    }

    /// Two edges `e1: a1 -> b1` and `e2: a2 -> b2` merged into one edge
    /// `E: m1 -> m2`. `Pi(E)` sends `(x1, x2)` to `(e1 x1, e2 x2)`, so two
    /// product rows are related exactly when both components are: the
    /// relation is the intersection of the edges' fan-outs, not their union,
    /// and a carried edge without pairs empties it.
    #[test]
    fn merged_edges_relate_product_rows_componentwise() {
        let edge = |src: &str, tgt: &str| Edge {
            src: src.into(),
            tgt: tgt.into(),
            kind: "fk".into(),
            name: Some("e".into()),
        };
        let two = |col: &str| {
            vec![
                HashMap::from([(col.to_owned(), Value::Int(0))]),
                HashMap::from([(col.to_owned(), Value::Int(1))]),
            ]
        };
        // a1 holds rows 0, 1; a2 rows 2, 3; b1 rows 4, 5; b2 rows 6, 7.
        let base = FInstance::new()
            .with_table("a1", two("a1"))
            .and_then(|f| f.with_table("a2", two("a2")))
            .and_then(|f| f.with_table("b1", two("b1")))
            .and_then(|f| f.with_table("b2", two("b2")))
            .and_then(|f| {
                f.with_foreign_key(
                    edge("a1", "b1"),
                    [(RowId(0), RowId(4)), (RowId(1), RowId(5))],
                )
            })
            .unwrap();
        let inst = base
            .clone()
            .with_foreign_key(
                edge("a2", "b2"),
                [(RowId(2), RowId(7)), (RowId(3), RowId(6))],
            )
            .unwrap();
        let mut migration = CompiledMigration::default();
        migration
            .surviving_verts
            .extend([Name::from("m1"), Name::from("m2")]);
        for (src, tgt) in [("a1", "m1"), ("a2", "m1"), ("b1", "m2"), ("b2", "m2")] {
            migration
                .vertex_remap
                .insert(Name::from(src), Name::from(tgt));
        }
        migration
            .edge_remap
            .insert(edge("a1", "b1"), edge("m1", "m2"));
        migration
            .edge_remap
            .insert(edge("a2", "b2"), edge("m1", "m2"));

        // m1's rows: 8 = (0, 2), 9 = (0, 3), 10 = (1, 2), 11 = (1, 3); m2's:
        // 12 = (4, 6), 13 = (4, 7), 14 = (5, 6), 15 = (5, 7).
        let (pi, _) = run_pi(&inst, &migration, &[], 100).unwrap();
        let r = |a: u32, b: u32| (RowId(a), RowId(b));
        assert_eq!(
            pi.pairs(&edge("m1", "m2")),
            &BTreeSet::from([r(8, 13), r(9, 12), r(10, 15), r(11, 14)]),
        );

        let (without, _) = run_pi(&base, &migration, &[], 100).unwrap();
        assert!(without.pairs(&edge("m1", "m2")).is_empty());
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

    /// A two-component fibre `{a, b} -> m` whose rows `0` (in `a`) and `1`
    /// (in `b`) both hold the column `x`.
    fn shared_column_fibre(left: Value, right: Value) -> (FInstance, CompiledMigration) {
        let inst = FInstance::new()
            .with_rows("a", [(RowId(0), HashMap::from([("x".to_owned(), left)]))])
            .and_then(|f| f.with_rows("b", [(RowId(1), HashMap::from([("x".to_owned(), right)]))]))
            .unwrap();
        let mut migration = CompiledMigration::default();
        migration.surviving_verts.insert(Name::from("m"));
        migration
            .vertex_remap
            .insert(Name::from("a"), Name::from("m"));
        migration
            .vertex_remap
            .insert(Name::from("b"), Name::from("m"));
        (inst, migration)
    }

    /// Fibre components agree on a shared column exactly when the cells
    /// have the same canonical bytes, so two `NaN` cells agree.
    #[test]
    fn components_holding_nan_agree() {
        let (inst, migration) = shared_column_fibre(Value::Float(f64::NAN), Value::Float(f64::NAN));
        let (pi, _) = run_pi(&inst, &migration, &[], 100).unwrap();
        let rows = pi.rows("m");
        assert_eq!(rows.len(), 1);
        assert!(matches!(rows[0].1.get("x"), Some(Value::Float(v)) if v.is_nan()));
    }

    /// `0.0` and `-0.0` are different cells, so components holding them
    /// collide rather than merging into whichever came last.
    #[test]
    fn components_holding_signed_zeros_collide() {
        let (inst, migration) = shared_column_fibre(Value::Float(0.0), Value::Float(-0.0));
        let err = run_pi(&inst, &migration, &[], 100).unwrap_err();
        assert!(
            matches!(&err, RestrictError::AttributeCollision { vertex, column } if vertex == "m" && column == "x"),
            "{err:?}"
        );
    }

    /// Pi is computed over the image of `F`: a target vertex no source vertex
    /// maps to has no table, as the rustdoc of [`functor_pi_with_hom`]
    /// states.
    #[test]
    fn a_target_vertex_outside_the_image_gets_no_table() {
        let mut migration = CompiledMigration::default();
        migration.surviving_verts.insert("a".into());
        let inst = FInstance::new()
            .with_rows("a", [(RowId(0), HashMap::new())])
            .unwrap();
        let pi = functor_pi(
            &inst,
            &make_test_schema(&["a"], &[]),
            &make_test_schema(&["a", "t"], &[]),
            &migration,
            100,
        )
        .unwrap();
        assert_eq!(pi.table_names(), vec![&Name::from("a")]);
        assert_eq!(pi.row_count("t"), 0);
    }
}
