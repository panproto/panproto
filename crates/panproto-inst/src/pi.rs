//! Right Kan extension (`Pi_F`) for instances.
//!
//! For set-valued functor instances, whose foreign keys are relations, the
//! right Kan extension is taken on the typed-graph category of each schema:
//! at a target vertex it is the product of the fibre's tables cut down to
//! the tuples the foreign keys inside the fibre relate, and at a target
//! edge the pairs of those rows that every edge carried to it relates. For
//! W-type instances, only vertex-injective migrations are
//! supported; a non-injective map is rejected rather than silently
//! producing `Sigma`-shaped output mislabeled as `Pi`.

use std::collections::{BTreeMap, HashMap};

use panproto_gat::Name;
use panproto_schema::{Edge, Schema};

use crate::error::RestrictError;
use crate::functor::{FInstance, Row, allocate_row, row_allocator, same_value};
use crate::instance_hom::FInstanceHom;
use crate::metadata::Node;
use crate::view_ref::RowId;
use crate::wtype::{CompiledMigration, WInstance, reconstruct_fans, resolve_edge};

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
/// Pi reads every foreign key as a relation, a set of pairs that may be
/// partial and many-valued, so an instance is a functor on the typed-graph
/// category of its schema: its objects are the schema's vertices and edges,
/// and each edge `e: a -> b` is an object with two arrows, to `a` and to
/// `b`. The instance sends a vertex to its rows and an edge to its pairs,
/// with the two projections. `F` is the functor the migration states
/// between the typed-graph categories of the two schemas: it sends each
/// source vertex (read as every table operation reads them; see
/// [`crate::functor_restrict`]) to its image, each source edge the
/// migration carries to its image edge and the edge's two arrows to the
/// image's, and each other edge of `src_schema` with both endpoints in one
/// fibre (an internal arrow) to that fibre's vertex, both its arrows to the
/// identity. `Pi_F(I)` at an object `d` is the limit of `I` over the comma
/// category `d/F`:
///
/// - At a target vertex `d`, whose only arrow out is its identity, the
///   objects of `d/F` are the vertices and internal arrows of `d`'s fibre.
///   The table of `d` is the product of the fibre's tables (components in
///   ascending name) cut down to the tuples every internal arrow relates:
///   the internal foreign key holds the pair of the tuple's rows at its two
///   ends.
/// - At a target edge `E: d -> d'`, the objects of `E/F` are the edges
///   carried to `E` together with the objects of `d/F` and `d'/F`, reached
///   along `E`'s two arrows. The pairs of `E` are the pairs `(x, y)` of a
///   row `x` of `d` and a row `y` of `d'` such that every edge carried to
///   `E` holds the pair of `x`'s row at its source and `y`'s row at its
///   target, so two parallel edges carried to one image take the
///   intersection of their pairs. With no edge carried to `E`, every pair
///   of rows is in the limit.
///
/// Nothing outside a vertex's own fibre constrains its table, so a row with
/// no pair on some edge keeps its place, `Pi_F` is monotone in `I`, and
/// along the identity migration it is the identity. Every comma category
/// is finite, so every limit is.
///
/// A target vertex outside the image of `F` is read as an attribute (a
/// column vertex; tables are keyed only by table vertices), not as a table:
/// the limit there, over an empty diagram, would be a table of one row
/// with no cells. It gets no table and the edges at it get no pairs. The
/// pairs of an edge outside `F`'s domain are not carried.
///
/// Row ids: the rows of a vertex whose fibre is one component keep the ids
/// of the rows they project to (the projection is injective). Every other
/// row of the result is fresh, allocated from the least id above every row
/// of the input, the target vertices taken in ascending name and each
/// vertex's rows in the product's order (the last component varying
/// fastest).
///
/// The returned map sends each source vertex `s` of a fibre to the
/// projection of its target's rows onto the rows of `s`, as an
/// [`FInstanceHom`] whose table map sends the target table `F(s)` to `s`.
///
/// # Errors
///
/// Returns [`RestrictError::NoEdgeFound`] when the migration carries an
/// edge of `src_schema` to an image that does not join the images of its
/// endpoints (naming those images: the migration states no functor) or to
/// an edge `tgt_schema` does not have (naming the image's endpoints), the
/// edges taken in ascending order; [`RestrictError::ProductSizeExceeded`]
/// when the product of the tables of a fibre of several components exceeds
/// `max_product_size`, or when a target edge no edge is carried to would
/// hold more than `max_product_size` pairs (reported at the edge's source
/// vertex); [`RestrictError::AttributeCollision`] when two components of
/// one row disagree on a shared column; and
/// [`RestrictError::RowIdsExhausted`] when the fresh row ids run out. The
/// migration is checked first, then each target vertex's product and
/// cells, the vertices in ascending name, then the target edges in
/// ascending order, and the first failure is returned.
pub fn functor_pi_with_hom(
    instance: &FInstance,
    src_schema: &Schema,
    tgt_schema: &Schema,
    migration: &CompiledMigration,
    max_product_size: usize,
) -> Result<(FInstance, BTreeMap<Name, FInstanceHom>), RestrictError> {
    crate::adjunction::check_edge_endpoints(migration)?;
    let edge_images = crate::adjunction::edge_images(migration);
    check_carried_edges(src_schema, tgt_schema, &edge_images)?;
    let fiber_map = build_fiber_map(migration);

    // The limit at every image vertex, before any row is created.
    let mut limits: BTreeMap<&Name, VertexLimit> = BTreeMap::new();
    for (vertex, components) in &fiber_map {
        let internal = internal_arrows(src_schema, &edge_images, components);
        let states = fibre_states(instance, vertex, components, &internal, max_product_size)?;
        let rows = states
            .iter()
            .map(|state| merged_row(instance, vertex, state))
            .collect::<Result<Vec<Row>, RestrictError>>()?;
        limits.insert(
            vertex,
            VertexLimit {
                components,
                states,
                rows,
            },
        );
    }

    // The limit at every target edge between image vertices, as pairs of
    // state indices.
    let mut carried_to: HashMap<&Edge, Vec<&Edge>> = HashMap::new();
    for (edge, image) in &edge_images {
        carried_to.entry(image).or_default().push(edge);
    }
    let mut target_edges: Vec<&Edge> = tgt_schema
        .edges
        .keys()
        .filter(|edge| limits.contains_key(&edge.src) && limits.contains_key(&edge.tgt))
        .collect();
    target_edges.sort_unstable();
    let mut edge_limits: Vec<(&Edge, Vec<(usize, usize)>)> = Vec::with_capacity(target_edges.len());
    for edge in target_edges {
        let mut carried = carried_to.get(edge).cloned().unwrap_or_default();
        carried.sort_unstable();
        let pairs = edge_limit(
            instance,
            edge,
            &limits[&edge.src],
            &limits[&edge.tgt],
            &carried,
            max_product_size,
        )?;
        edge_limits.push((edge, pairs));
    }

    let mut allocator = row_allocator(instance.row_ids());
    let mut out = FInstance::new();
    let mut projections: BTreeMap<Name, FInstanceHom> = BTreeMap::new();
    let mut ids: HashMap<&Name, Vec<RowId>> = HashMap::new();
    for (&vertex, limit) in &limits {
        let keeps_ids = limit.components.len() == 1;
        let mut placed = Vec::with_capacity(limit.states.len());
        for (state, row) in limit.states.iter().zip(&limit.rows) {
            let id = if keeps_ids {
                state[0]
            } else {
                allocate_row(&mut allocator).map_err(|_| RestrictError::RowIdsExhausted)?
            };
            for (component, &src_row) in limit.components.iter().zip(state) {
                projections
                    .entry(component.clone())
                    .or_default()
                    .row_map
                    .insert(id, src_row);
            }
            out.place_row(vertex.clone(), id, row.clone());
            placed.push(id);
        }
        ids.insert(vertex, placed);
    }
    for (edge, pairs) in edge_limits {
        let (from, to) = (&ids[&edge.src], &ids[&edge.tgt]);
        for (x, y) in pairs {
            out.place_pair(edge.clone(), from[x], to[y]);
        }
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

    Ok((out, projections))
}

/// The limit of an instance at one image vertex: the tuples of the fibre's
/// rows the internal arrows relate, and the row each becomes.
struct VertexLimit<'a> {
    /// The fibre components, ascending.
    components: &'a [Name],
    /// The tuples, one row of each component, in the product's order.
    states: Vec<Vec<RowId>>,
    /// The row of each tuple: the union of its components' cells.
    rows: Vec<Row>,
}

impl VertexLimit<'_> {
    /// The position of a source vertex among the components.
    fn position(&self, vertex: &Name) -> Option<usize> {
        self.components.iter().position(|c| c == vertex)
    }

    /// The tuples holding each row of the component at `position`.
    fn by_row(&self, position: usize) -> HashMap<RowId, Vec<usize>> {
        let mut index: HashMap<RowId, Vec<usize>> = HashMap::new();
        for (state, rows) in self.states.iter().enumerate() {
            index.entry(rows[position]).or_default().push(state);
        }
        index
    }
}

/// The pairs of the limit at a target edge `E: d -> d'`, as pairs of tuple
/// indices of `from` (the limit at `d`) and `to` (at `d'`), ascending: the
/// pairs every edge of `carried` (the source edges carried to `E`,
/// ascending) holds between the two tuples' rows at its ends, or every pair
/// when no edge is carried to `E`.
///
/// # Errors
///
/// Returns [`RestrictError::ProductSizeExceeded`], at `E`'s source vertex,
/// when no edge is carried to `E` and the pairs of rows exceed
/// `max_product_size`.
fn edge_limit(
    instance: &FInstance,
    edge: &Edge,
    from: &VertexLimit<'_>,
    to: &VertexLimit<'_>,
    carried: &[&Edge],
    max_product_size: usize,
) -> Result<Vec<(usize, usize)>, RestrictError> {
    // `check_edge_endpoints` refused every edge off its endpoints, so each
    // carried edge runs from a component of `from` to one of `to`.
    let constraints: Vec<(&Edge, usize, usize)> = carried
        .iter()
        .filter_map(|&e| Some((e, from.position(&e.src)?, to.position(&e.tgt)?)))
        .collect();
    let Some((&(first, src_at, tgt_at), rest)) = constraints.split_first() else {
        let size = from.states.len().saturating_mul(to.states.len());
        if size > max_product_size {
            return Err(RestrictError::ProductSizeExceeded {
                vertex: edge.src.to_string(),
                actual: size,
                limit: max_product_size,
            });
        }
        return Ok((0..from.states.len())
            .flat_map(|x| (0..to.states.len()).map(move |y| (x, y)))
            .collect());
    };
    let to_by_row = to.by_row(tgt_at);
    let mut pairs = Vec::new();
    for (x, state) in from.states.iter().enumerate() {
        let mut ys: Vec<usize> = instance
            .targets_of(first, state[src_at])
            .flat_map(|row| to_by_row.get(&row).into_iter().flatten().copied())
            .filter(|&y| {
                let candidate = &to.states[y];
                rest.iter()
                    .all(|&(e, s, t)| instance.contains_pair(e, state[s], candidate[t]))
            })
            .collect();
        ys.sort_unstable();
        pairs.extend(ys.into_iter().map(|y| (x, y)));
    }
    Ok(pairs)
}

/// The row of a tuple of the limit at `vertex`: the union of the cells of
/// its components' rows.
///
/// # Errors
///
/// Returns [`RestrictError::AttributeCollision`] when two components hold
/// the same column with different values (by canonical bytes), which the
/// union would otherwise settle by overwriting one with the other.
fn merged_row(
    instance: &FInstance,
    vertex: &Name,
    components: &[RowId],
) -> Result<Row, RestrictError> {
    let mut merged = Row::new();
    for &id in components {
        for (col, val) in instance.row(id).into_iter().flatten() {
            if let Some(prior) = merged.get(col) {
                if !same_value(prior, val) {
                    return Err(RestrictError::AttributeCollision {
                        vertex: vertex.to_string(),
                        column: col.clone(),
                    });
                }
            }
            merged.insert(col.clone(), val.clone());
        }
    }
    Ok(merged)
}

/// The tuples of a fibre: the assignments of a row to every component that
/// the internal arrows relate, in the product's order (components in the
/// order given, the last varying fastest).
///
/// # Errors
///
/// Returns [`RestrictError::ProductSizeExceeded`] when the fibre has several
/// components and the product of their tables exceeds `max_product_size`.
fn fibre_states(
    instance: &FInstance,
    vertex: &Name,
    components: &[Name],
    internal: &[&Edge],
    max_product_size: usize,
) -> Result<Vec<Vec<RowId>>, RestrictError> {
    let tables: Vec<Vec<RowId>> = components
        .iter()
        .map(|c| instance.rows(c).into_iter().map(|(id, _)| id).collect())
        .collect();
    if tables.len() > 1 {
        let product_size = tables
            .iter()
            .try_fold(1_usize, |acc, rows| acc.checked_mul(rows.len()))
            .unwrap_or(usize::MAX);
        if product_size > max_product_size {
            return Err(RestrictError::ProductSizeExceeded {
                vertex: vertex.to_string(),
                actual: product_size,
                limit: max_product_size,
            });
        }
    }
    let position: HashMap<&Name, usize> =
        components.iter().enumerate().map(|(i, c)| (c, i)).collect();
    let mut states: Vec<Vec<RowId>> = vec![Vec::new()];
    for rows in &tables {
        let mut next = Vec::with_capacity(states.len() * rows.len());
        for state in &states {
            for &row in rows {
                let mut extended = state.clone();
                extended.push(row);
                next.push(extended);
            }
        }
        states = next;
    }
    states.retain(|state| {
        internal.iter().all(|edge| {
            instance.contains_pair(edge, state[position[&edge.src]], state[position[&edge.tgt]])
        })
    });
    Ok(states)
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
        // row 0 references other row 4 and row 1 references row 3.
        let inst = FInstance::new()
            .with_table("src_a", rows_a)
            .and_then(|f| f.with_table("src_c", rows_c))
            .and_then(|f| f.with_table("other", rows_o))
            .and_then(|f| {
                f.with_foreign_key(
                    fk_edge.clone(),
                    [(RowId(0), RowId(4)), (RowId(1), RowId(3))],
                )
            })
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
            &BTreeSet::from([(RowId(5), RowId(4)), (RowId(6), RowId(3))])
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
        // row 5; src_a row 0 references src_b row 4 and row 1 row 3.
        let inst = FInstance::new()
            .with_table("src_a", rows_a)
            .and_then(|f| f.with_table("src_a2", rows_p))
            .and_then(|f| f.with_table("src_b", rows_b))
            .and_then(|f| f.with_table("src_b2", rows_q))
            .and_then(|f| {
                f.with_foreign_key(
                    fk_edge.clone(),
                    [(RowId(0), RowId(4)), (RowId(1), RowId(3))],
                )
            })
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
            &BTreeSet::from([(RowId(6), RowId(9)), (RowId(7), RowId(8))])
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
    /// on the target: the table of `t` is the product of the fibre's tables,
    /// and the loop holds a pair of tuples exactly when both carried loops
    /// hold the pairs of their components.
    #[test]
    fn carried_loops_in_a_two_vertex_fibre_take_the_limit() {
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
        let (pi, _) = run_pi(&inst, &migration, &[], 100).unwrap();
        // One tuple, (0, 1), at the fresh id 2.
        assert_eq!(
            pi.table("t").map(crate::functor::Table::ids),
            Some(vec![RowId(2)])
        );
        assert_eq!(
            pi.pairs(&self_edge("t")),
            &BTreeSet::from([(RowId(2), RowId(2))])
        );
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

    /// An interim rejection of such fibres until the equaliser lands is moot:
    /// over a fibre with an internal edge, Pi returns the equalised limit,
    /// never the bare product. Made total, the internal foreign key keeps the
    /// two tuples it relates, `(a0, b1)` and `(a1, b0)`, of the four.
    #[test]
    fn pi_dispatch_rejects_a_fibre_with_an_internal_edge() {
        let (inst, migration, fk) = intra_fibre_fixture();
        let total = inst.with_foreign_key(fk, [(RowId(1), RowId(2))]).unwrap();
        let (pi, projections) = run_pi(&total, &migration, &[], 100).unwrap();
        let mut drawn_rows: Vec<(RowId, RowId)> = pi
            .rows("m")
            .into_iter()
            .map(|(id, _)| {
                (
                    projections[&Name::from("src_a")].row_map[&id],
                    projections[&Name::from("src_b")].row_map[&id],
                )
            })
            .collect();
        drawn_rows.sort_unstable();
        assert_eq!(drawn_rows, vec![(RowId(0), RowId(3)), (RowId(1), RowId(2))]);
    }

    /// The intra-fibre foreign key `{(a0, b1)}` carried to a loop on `m`: it
    /// is no arrow of the fibre, so the table of `m` is the whole product,
    /// and the loop holds the pairs of tuples `(x, y)` whose `src_a` row and
    /// `src_b` row the foreign key relates: `x` draws `a0`, `y` draws `b1`.
    /// The foreign key is a relation, so this fan-out is the limit.
    #[test]
    fn an_edge_carried_to_a_loop_relates_the_tuples_its_pairs_join() {
        let (inst, mut migration, fk) = intra_fibre_fixture();
        migration.edge_remap.insert(fk, loop_on("m", "ref"));
        let (pi, projections) = run_pi(&inst, &migration, &[], 100).unwrap();
        // 4 = (a0, b0), 5 = (a0, b1), 6 = (a1, b0), 7 = (a1, b1).
        assert_eq!(
            drawn(&pi, &projections, "m", "src_a"),
            vec![
                (RowId(4), RowId(0)),
                (RowId(5), RowId(0)),
                (RowId(6), RowId(1)),
                (RowId(7), RowId(1))
            ]
        );
        assert_eq!(
            drawn(&pi, &projections, "m", "src_b"),
            vec![
                (RowId(4), RowId(2)),
                (RowId(5), RowId(3)),
                (RowId(6), RowId(2)),
                (RowId(7), RowId(3))
            ]
        );
        assert_eq!(
            pi.pairs(&loop_on("m", "ref")),
            &BTreeSet::from([
                (RowId(4), RowId(5)),
                (RowId(4), RowId(7)),
                (RowId(5), RowId(5)),
                (RowId(5), RowId(7))
            ])
        );
    }

    /// The same foreign key listed as surviving: the migration relabels it to
    /// the loop on `m`, so Pi takes the same limit as for the remapped one.
    #[test]
    fn a_surviving_intra_fibre_edge_travels_to_its_loop() {
        let (inst, mut remapped, fk) = intra_fibre_fixture();
        let mut surviving = remapped.clone();
        surviving.surviving_edges.insert(fk.clone());
        remapped.edge_remap.insert(fk.clone(), loop_on("m", "ref"));
        let src = make_test_schema(&[], std::slice::from_ref(&fk));
        let tgt = make_test_schema(&[], &[loop_on("m", "ref")]);
        assert_eq!(
            functor_pi(&inst, &src, &tgt, &surviving, 100).unwrap(),
            functor_pi(&inst, &src, &tgt, &remapped, 100).unwrap()
        );
    }

    /// A one-vertex fibre `{a}` whose loop is relabelled onto `t`: the table
    /// of `t` is `a`'s, its rows keep their ids, and the loop travels with
    /// its pairs.
    #[test]
    fn a_one_vertex_fibre_carries_its_loop() {
        let cell = |v: i64| HashMap::from([("n".to_owned(), Value::Int(v))]);
        let inst = FInstance::new()
            .with_table("a", vec![cell(0), cell(1)])
            .and_then(|f| {
                f.with_foreign_key(
                    loop_on("a", "next"),
                    [(RowId(0), RowId(1)), (RowId(1), RowId(1))],
                )
            })
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
            &BTreeSet::from([(RowId(0), RowId(1)), (RowId(1), RowId(1))])
        );
    }

    /// The same loop without a pair at row 1: a foreign key is a relation,
    /// so row 1 keeps its place, and the loop holds the one pair it held.
    #[test]
    fn a_row_without_a_pair_on_a_carried_edge_keeps_its_place() {
        let inst = FInstance::new()
            .with_table("a", blank(2))
            .and_then(|f| f.with_foreign_key(loop_on("a", "next"), [(RowId(0), RowId(1))]))
            .unwrap();
        let migration = migration_of(
            &[("a", "t")],
            &[(loop_on("a", "next"), loop_on("t", "next"))],
        );
        let (pi, _) = run_pi(&inst, &migration, &[], 100).unwrap();
        assert_eq!(
            pi.table("t").map(crate::functor::Table::ids),
            Some(vec![RowId(0), RowId(1)])
        );
        assert_eq!(
            pi.pairs(&loop_on("t", "next")),
            &BTreeSet::from([(RowId(0), RowId(1))])
        );
    }

    /// Two loops of a one-vertex fibre carried to one loop: the table of `t`
    /// is `a`'s, and the loop holds the pairs both loops hold, `(0, 0)` (`p`
    /// holds `(1, 1)`, `q` holds `(1, 0)`).
    #[test]
    fn loops_carried_to_one_image_take_the_intersection_of_their_pairs() {
        let inst = FInstance::new()
            .with_table("a", blank(2))
            .and_then(|f| {
                f.with_foreign_key(
                    loop_on("a", "p"),
                    [(RowId(0), RowId(0)), (RowId(1), RowId(1))],
                )
            })
            .and_then(|f| {
                f.with_foreign_key(
                    loop_on("a", "q"),
                    [(RowId(0), RowId(0)), (RowId(1), RowId(0))],
                )
            })
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
        let (pi, _) = run_pi(&inst, &migration, &[], 100).unwrap();
        assert_eq!(
            pi.table("t").map(crate::functor::Table::ids),
            Some(vec![RowId(0), RowId(1)])
        );
        assert_eq!(
            pi.pairs(&loop_on("t", "p")),
            &BTreeSet::from([(RowId(0), RowId(0))])
        );
    }

    /// A one-vertex fibre with one loop sent to the identity and one carried
    /// to a loop: the table of `t` is the rows the identity loop relates to
    /// themselves, and the carried loop travels with its pairs.
    #[test]
    fn a_carried_loop_beside_a_loop_sent_to_the_identity_takes_the_limit() {
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
        let (pi, _) = run_pi(&inst, &migration, &[], 100).unwrap();
        assert_eq!(
            pi.table("t").map(crate::functor::Table::ids),
            Some(vec![RowId(0)])
        );
        assert_eq!(
            pi.pairs(&loop_on("t", "c")),
            &BTreeSet::from([(RowId(0), RowId(0))])
        );
    }

    /// Two edges `e1: a1 -> b1` and `e2: a2 -> b2` merged into one edge
    /// `E: m1 -> m2`: two product rows are related exactly when both
    /// components are, so the relation is the intersection of the edges'
    /// fan-outs, not their union, and a carried edge without pairs empties
    /// it.
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

    // --- Pi is the limit over the comma category d/F ---

    /// An edge `src -> tgt` of kind `fk` named `name`.
    fn fk(src: &str, tgt: &str, name: &str) -> Edge {
        Edge {
            src: src.into(),
            tgt: tgt.into(),
            kind: "fk".into(),
            name: Some(name.into()),
        }
    }

    /// `n` rows without cells.
    fn blank(n: usize) -> Vec<Row> {
        vec![Row::new(); n]
    }

    /// A migration sending each `(source, target)` vertex and each
    /// `(source, target)` edge as listed, every target vertex surviving.
    fn migration_of(vertices: &[(&str, &str)], edges: &[(Edge, Edge)]) -> CompiledMigration {
        let mut migration = CompiledMigration::default();
        for &(src, tgt) in vertices {
            migration.vertex_remap.insert(src.into(), tgt.into());
            migration.surviving_verts.insert(tgt.into());
        }
        for (src, tgt) in edges {
            migration.edge_remap.insert(src.clone(), tgt.clone());
        }
        migration
    }

    /// Pi along `migration`, against the schemas of [`schemas`] with the
    /// target-only edges `extra` added to the target.
    fn pi_with_target_edges(
        inst: &FInstance,
        migration: &CompiledMigration,
        extra: &[Edge],
    ) -> Result<(FInstance, BTreeMap<Name, FInstanceHom>), RestrictError> {
        let (src, tgt) = schemas(inst, migration, &[]);
        let mut edges: Vec<Edge> = tgt.edges.keys().cloned().collect();
        edges.extend(extra.iter().cloned());
        functor_pi_with_hom(inst, &src, &make_test_schema(&[], &edges), migration, 100)
    }

    /// The rows of `vertex` with the component each drew from `component`.
    fn drawn(
        pi: &FInstance,
        projections: &BTreeMap<Name, FInstanceHom>,
        vertex: &str,
        component: &str,
    ) -> Vec<(RowId, RowId)> {
        pi.rows(vertex)
            .into_iter()
            .map(|(id, _)| (id, projections[&Name::from(component)].row_map[&id]))
            .collect()
    }

    /// Two components of one fibre carried along the same target edge into
    /// one component: the table of `m1` is the product `I(a1) x I(a2)`, and
    /// `E` holds a pair `(x, y)` exactly when both `e1` and `e2` hold the
    /// pairs of `x`'s components at `y`'s row of `b`.
    #[test]
    fn edges_carried_to_one_image_relate_where_both_hold() {
        // a1 holds 0, 1; a2 holds 2, 3; b holds 4, 5.
        let inst = FInstance::new()
            .with_table("a1", blank(2))
            .and_then(|f| f.with_table("a2", blank(2)))
            .and_then(|f| f.with_table("b", blank(2)))
            .and_then(|f| {
                f.with_foreign_key(
                    fk("a1", "b", "e1"),
                    [(RowId(0), RowId(4)), (RowId(1), RowId(5))],
                )
            })
            .and_then(|f| {
                f.with_foreign_key(
                    fk("a2", "b", "e2"),
                    [(RowId(2), RowId(4)), (RowId(3), RowId(5))],
                )
            })
            .unwrap();
        let migration = migration_of(
            &[("a1", "m1"), ("a2", "m1"), ("b", "m2")],
            &[
                (fk("a1", "b", "e1"), fk("m1", "m2", "E")),
                (fk("a2", "b", "e2"), fk("m1", "m2", "E")),
            ],
        );
        let (pi, projections) = run_pi(&inst, &migration, &[], 100).unwrap();
        // m1's rows are fresh: 6 = (0, 2), 7 = (0, 3), 8 = (1, 2),
        // 9 = (1, 3); `b` keeps its ids.
        assert_eq!(
            drawn(&pi, &projections, "m1", "a1"),
            vec![
                (RowId(6), RowId(0)),
                (RowId(7), RowId(0)),
                (RowId(8), RowId(1)),
                (RowId(9), RowId(1))
            ]
        );
        assert_eq!(
            pi.pairs(&fk("m1", "m2", "E")),
            &BTreeSet::from([(RowId(6), RowId(4)), (RowId(9), RowId(5))])
        );
    }

    /// Two parallel edges carried to one: the table of `m1` is `a`'s, and
    /// `E` holds the pairs both edges hold.
    #[test]
    fn parallel_edges_carried_to_one_take_the_intersection() {
        // a holds 0, 1; b holds 2, 3.
        let inst = FInstance::new()
            .with_table("a", blank(2))
            .and_then(|f| f.with_table("b", blank(2)))
            .and_then(|f| {
                f.with_foreign_key(
                    fk("a", "b", "e"),
                    [(RowId(0), RowId(2)), (RowId(1), RowId(3))],
                )
            })
            .and_then(|f| {
                f.with_foreign_key(
                    fk("a", "b", "f"),
                    [(RowId(0), RowId(2)), (RowId(1), RowId(2))],
                )
            })
            .unwrap();
        let migration = migration_of(
            &[("a", "m1"), ("b", "m2")],
            &[
                (fk("a", "b", "e"), fk("m1", "m2", "E")),
                (fk("a", "b", "f"), fk("m1", "m2", "E")),
            ],
        );
        let (pi, _) = run_pi(&inst, &migration, &[], 100).unwrap();
        assert_eq!(
            pi.table("m1").map(crate::functor::Table::ids),
            Some(vec![RowId(0), RowId(1)])
        );
        assert_eq!(
            pi.pairs(&fk("m1", "m2", "E")),
            &BTreeSet::from([(RowId(0), RowId(2))])
        );
    }

    /// A target edge no edge is carried to, between two image vertices:
    /// nothing constrains it, so it holds every pair of rows, and the tables
    /// at its ends are their fibres' own.
    #[test]
    fn a_target_only_edge_holds_every_pair_of_rows() {
        // a holds 0; b holds 1, 2.
        let inst = FInstance::new()
            .with_table("a", blank(1))
            .and_then(|f| f.with_table("b", blank(2)))
            .unwrap();
        let migration = migration_of(&[("a", "m1"), ("b", "m2")], &[]);
        let edge = fk("m1", "m2", "E");
        let (pi, _) = pi_with_target_edges(&inst, &migration, std::slice::from_ref(&edge)).unwrap();
        assert_eq!(
            pi.table("m1").map(crate::functor::Table::ids),
            Some(vec![RowId(0)])
        );
        assert_eq!(
            pi.pairs(&edge),
            &BTreeSet::from([(RowId(0), RowId(1)), (RowId(0), RowId(2))])
        );
    }

    /// A target-only loop over two rows holds all four pairs; with a bound
    /// below four, Pi refuses the pairs as a product too large, at the
    /// loop's vertex.
    #[test]
    fn a_target_only_loop_holds_every_pair_of_rows_up_to_the_bound() {
        let inst = FInstance::new().with_table("a", blank(2)).unwrap();
        let migration = migration_of(&[("a", "m")], &[]);
        let (pi, _) = pi_with_target_edges(&inst, &migration, &[loop_on("m", "L")]).unwrap();
        assert_eq!(
            pi.pairs(&loop_on("m", "L")),
            &BTreeSet::from([
                (RowId(0), RowId(0)),
                (RowId(0), RowId(1)),
                (RowId(1), RowId(0)),
                (RowId(1), RowId(1))
            ])
        );
        let src = make_test_schema(&[], &[]);
        let tgt = make_test_schema(&[], &[loop_on("m", "L")]);
        let err = functor_pi(&inst, &src, &tgt, &migration, 3).unwrap_err();
        assert!(
            matches!(&err, RestrictError::ProductSizeExceeded { vertex, actual: 4, limit: 3 } if vertex == "m"),
            "{err:?}"
        );
    }

    /// A carried edge into one component of a two-vertex target fibre: the
    /// table of `m2` is `I(b) x I(b2)`, and `E` relates `a`'s row to every
    /// tuple whose `b` row the edge holds, whatever its `b2` row.
    #[test]
    fn a_carried_edge_into_one_component_relates_every_tuple_holding_its_row() {
        // a holds 0; b holds 1; b2 holds 2, 3.
        let inst = FInstance::new()
            .with_table("a", blank(1))
            .and_then(|f| f.with_table("b", blank(1)))
            .and_then(|f| f.with_table("b2", blank(2)))
            .and_then(|f| f.with_foreign_key(fk("a", "b", "e"), [(RowId(0), RowId(1))]))
            .unwrap();
        let migration = migration_of(
            &[("a", "m1"), ("b", "m2"), ("b2", "m2")],
            &[(fk("a", "b", "e"), fk("m1", "m2", "E"))],
        );
        let (pi, projections) = run_pi(&inst, &migration, &[], 100).unwrap();
        // m1 keeps a's row 0; m2's rows are 4 = (1, 2) and 5 = (1, 3).
        assert_eq!(
            drawn(&pi, &projections, "m2", "b2"),
            vec![(RowId(4), RowId(2)), (RowId(5), RowId(3))]
        );
        assert_eq!(
            pi.pairs(&fk("m1", "m2", "E")),
            &BTreeSet::from([(RowId(0), RowId(4)), (RowId(0), RowId(5))])
        );
    }

    // --- foreign keys are relations: Pi on the typed-graph category ---

    /// The identity migration on `vertices`, every listed edge surviving,
    /// against one schema declaring those edges.
    fn identity_pi(
        inst: &FInstance,
        vertices: &[&str],
        edges: &[Edge],
    ) -> Result<(FInstance, BTreeMap<Name, FInstanceHom>), RestrictError> {
        let mut migration =
            migration_of(&vertices.iter().map(|&v| (v, v)).collect::<Vec<_>>(), &[]);
        migration.surviving_edges = edges.iter().cloned().collect();
        let schema = make_test_schema(vertices, edges);
        functor_pi_with_hom(inst, &schema, &schema, &migration, 100)
    }

    /// Pi along the identity is the identity on a foreign key that is
    /// partial (post 3 has no author) and many-valued (post 4 has two).
    #[test]
    fn pi_along_the_identity_keeps_a_partial_and_many_valued_foreign_key() {
        let author = fk("post", "user", "author");
        let inst = FInstance::new()
            .with_rows("user", [(RowId(0), Row::new()), (RowId(1), Row::new())])
            .and_then(|f| {
                f.with_rows(
                    "post",
                    [
                        (RowId(2), Row::new()),
                        (RowId(3), Row::new()),
                        (RowId(4), Row::new()),
                    ],
                )
            })
            .and_then(|f| {
                f.with_foreign_key(
                    author.clone(),
                    [
                        (RowId(2), RowId(0)),
                        (RowId(4), RowId(0)),
                        (RowId(4), RowId(1)),
                    ],
                )
            })
            .unwrap();
        let (pi, _) = identity_pi(&inst, &["post", "user"], &[author]).unwrap();
        assert_eq!(pi, inst);
    }

    /// Pi along the identity keeps an org chart whose root has no manager:
    /// a nullable self-reference empties nothing.
    #[test]
    fn pi_along_the_identity_keeps_a_nullable_self_reference() {
        let manager = fk("emp", "emp", "manager");
        let inst = FInstance::new()
            .with_table("emp", blank(4))
            .and_then(|f| {
                f.with_foreign_key(
                    manager.clone(),
                    [
                        (RowId(1), RowId(0)),
                        (RowId(2), RowId(0)),
                        (RowId(3), RowId(1)),
                    ],
                )
            })
            .unwrap();
        let (pi, _) = identity_pi(&inst, &["emp"], &[manager]).unwrap();
        assert_eq!(pi, inst);
    }

    /// The fibre tuples of every row of `vertex`: for each row, the row of
    /// each fibre component it projects to, by component name.
    fn tuples(
        pi: &FInstance,
        projections: &BTreeMap<Name, FInstanceHom>,
        vertex: &str,
    ) -> BTreeSet<Vec<(Name, RowId)>> {
        pi.rows(vertex)
            .into_iter()
            .map(|(id, _)| {
                projections
                    .iter()
                    .filter_map(|(component, hom)| {
                        hom.row_map.get(&id).map(|&row| (component.clone(), row))
                    })
                    .collect()
            })
            .collect()
    }

    /// Adding a row to the input never removes a row of the output: a
    /// target vertex whose fibre gains its first row is no longer read
    /// differently from one that held rows all along.
    #[test]
    fn pi_is_monotone_when_a_fibre_gains_its_first_row() {
        // A target edge with no preimage between m1 and the two-vertex
        // fibre {b1, b2} of m2.
        let migration = migration_of(&[("a", "m1"), ("b1", "m2"), ("b2", "m2")], &[]);
        let edge = fk("m1", "m2", "E");
        let i = FInstance::new().with_table("a", blank(1)).unwrap();
        let j = i.clone().with_table("b1", blank(1)).unwrap();
        let (pi_i, proj_i) =
            pi_with_target_edges(&i, &migration, std::slice::from_ref(&edge)).unwrap();
        let (pi_j, proj_j) =
            pi_with_target_edges(&j, &migration, std::slice::from_ref(&edge)).unwrap();
        for vertex in ["m1", "m2"] {
            assert!(
                tuples(&pi_i, &proj_i, vertex).is_subset(&tuples(&pi_j, &proj_j, vertex)),
                "{vertex}: {:?} then {:?}",
                tuples(&pi_i, &proj_i, vertex),
                tuples(&pi_j, &proj_j, vertex)
            );
        }

        // A carried edge e: a -> b, and a row of a with no e pair.
        let carried = migration_of(
            &[("a", "m1"), ("b", "m2")],
            &[(fk("a", "b", "e"), fk("m1", "m2", "E"))],
        );
        let j = i.clone().with_table("b", blank(1)).unwrap();
        let (pi_i, proj_i) = run_pi(&i, &carried, &[], 100).unwrap();
        let (pi_j, proj_j) = run_pi(&j, &carried, &[], 100).unwrap();
        for vertex in ["m1", "m2"] {
            assert!(
                tuples(&pi_i, &proj_i, vertex).is_subset(&tuples(&pi_j, &proj_j, vertex)),
                "{vertex}: {:?} then {:?}",
                tuples(&pi_i, &proj_i, vertex),
                tuples(&pi_j, &proj_j, vertex)
            );
        }
    }

    /// A brute-force oracle for `Pi_F`: it lists the comma category `d/F`
    /// of every target object `d` of the typed-graph category, object by
    /// object, and enumerates the cones.
    mod limit_oracle {
        use std::collections::{BTreeMap, BTreeSet};

        use proptest::prelude::*;

        use super::{blank, fk, make_test_schema, tuples};
        use crate::functor::FInstance;
        use crate::instance_hom::FInstanceHom;
        use crate::view_ref::RowId;
        use crate::wtype::CompiledMigration;
        use panproto_gat::Name;
        use panproto_schema::Edge;

        /// A small linear congruential generator, so one seed fixes a case.
        struct Lcg(u64);

        impl Lcg {
            fn below(&mut self, n: usize) -> usize {
                self.0 = self
                    .0
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                usize::try_from(self.0 >> 33).unwrap() % n
            }
        }

        /// A case: the instance, the migration, the vertex map, the source
        /// edges sent to the identity, the carried edges with their images
        /// and every target edge. Target edges may form cycles and loops,
        /// and a target vertex may lie outside the image.
        struct Case {
            inst: FInstance,
            migration: CompiledMigration,
            vertex_map: BTreeMap<String, String>,
            internal: Vec<Edge>,
            carried: Vec<(Edge, Edge)>,
            target_edges: Vec<Edge>,
        }

        /// The source vertices of each image vertex, ascending.
        fn fibres(case: &Case) -> BTreeMap<String, Vec<String>> {
            let mut fibres: BTreeMap<String, Vec<String>> = BTreeMap::new();
            for (src, tgt) in &case.vertex_map {
                fibres.entry(tgt.clone()).or_default().push(src.clone());
            }
            fibres
        }

        /// Random pairs between the rows of `a` and `b`: a total function
        /// (each row of `a` to one row of `b`) or, one time in two, a
        /// relation (each pair kept with probability one half).
        fn random_pairs(r: &mut Lcg, inst: &FInstance, a: &str, b: &str) -> Vec<(RowId, RowId)> {
            let ids =
                |v: &str| -> Vec<RowId> { inst.rows(v).into_iter().map(|(id, _)| id).collect() };
            let (srcs, tgts) = (ids(a), ids(b));
            let relation = r.below(2) == 0;
            let mut pairs = Vec::new();
            for &s in &srcs {
                if relation {
                    for &t in &tgts {
                        if r.below(2) == 0 {
                            pairs.push((s, t));
                        }
                    }
                } else if !tgts.is_empty() {
                    pairs.push((s, tgts[r.below(tgts.len())]));
                }
            }
            pairs
        }

        fn case(seed: u64) -> Case {
            let mut r = Lcg(seed);
            let sources = 1 + r.below(4);
            let targets = 1 + r.below(3);
            let mut vertex_map = BTreeMap::new();
            let mut inst = FInstance::new();
            for i in 0..sources {
                let name = format!("s{i}");
                vertex_map.insert(name.clone(), format!("t{}", r.below(targets)));
                inst = inst.with_table(name, blank(r.below(3))).unwrap();
            }
            let (mut internal, mut carried) = (Vec::new(), Vec::new());
            for k in 0..r.below(5) {
                let a = format!("s{}", r.below(sources));
                let b = format!("s{}", r.below(sources));
                let (ta, tb) = (vertex_map[&a].clone(), vertex_map[&b].clone());
                let edge = fk(&a, &b, &format!("e{k}"));
                if ta == tb && r.below(2) == 0 {
                    internal.push(edge.clone());
                } else {
                    carried.push((edge.clone(), fk(&ta, &tb, &format!("E{}", r.below(2)))));
                }
                let pairs = random_pairs(&mut r, &inst, &a, &b);
                inst = inst.with_foreign_key(edge, pairs).unwrap();
            }
            let mut target_edges: Vec<Edge> = carried.iter().map(|(_, e)| e.clone()).collect();
            for k in 0..r.below(3) {
                let (i, j) = (r.below(targets), r.below(targets));
                target_edges.push(fk(&format!("t{i}"), &format!("t{j}"), &format!("X{k}")));
            }
            target_edges.sort_unstable();
            target_edges.dedup();
            let mut migration = CompiledMigration::default();
            for (src, tgt) in &vertex_map {
                migration
                    .vertex_remap
                    .insert(src.as_str().into(), tgt.as_str().into());
                migration.surviving_verts.insert(tgt.as_str().into());
            }
            for (src, tgt) in &carried {
                migration.edge_remap.insert(src.clone(), tgt.clone());
            }
            Case {
                inst,
                migration,
                vertex_map,
                internal,
                carried,
                target_edges,
            }
        }

        /// Pi on a case, against schemas declaring its source and target
        /// edges.
        fn pi_of(case: &Case, inst: &FInstance) -> (FInstance, BTreeMap<Name, FInstanceHom>) {
            let mut src_edges = case.internal.clone();
            src_edges.extend(case.carried.iter().map(|(e, _)| e.clone()));
            super::functor_pi_with_hom(
                inst,
                &make_test_schema(&[], &src_edges),
                &make_test_schema(&[], &case.target_edges),
                &case.migration,
                1_000_000,
            )
            .unwrap()
        }
        /// The assignments of a row of its domain to every object that
        /// relate the rows at the two ends of every arrow, found object by
        /// object, each arrow checked once both its ends are assigned.
        fn enumerate_cones(
            inst: &FInstance,
            domains: &[Vec<RowId>],
            arrows: &[(usize, usize, Edge)],
        ) -> Vec<Vec<RowId>> {
            let objects = domains.len();
            // Enumerate the assignments, object by object, checking each
            // arrow once both its ends are assigned.
            let mut cones: Vec<Vec<RowId>> = Vec::new();
            let mut partial: Vec<RowId> = Vec::new();
            let mut choice: Vec<usize> = Vec::new();
            loop {
                if partial.len() == objects {
                    cones.push(partial.clone());
                } else if !domains[partial.len()].is_empty() {
                    choice.push(0);
                    partial.push(domains[partial.len()][0]);
                    let k = partial.len() - 1;
                    if arrows.iter().all(|(i, j, e)| {
                        (*i.max(j) != k) || inst.contains_pair(e, partial[*i], partial[*j])
                    }) {
                        continue;
                    }
                }
                // Advance the last choice, backtracking as needed.
                while let Some(last) = choice.pop() {
                    partial.pop();
                    let k = partial.len();
                    if last + 1 < domains[k].len() {
                        choice.push(last + 1);
                        partial.push(domains[k][last + 1]);
                        if arrows.iter().all(|(i, j, e)| {
                            (*i.max(j) != k) || inst.contains_pair(e, partial[*i], partial[*j])
                        }) {
                            break;
                        }
                    }
                }
                if choice.is_empty() {
                    break;
                }
            }
            cones
        }

        /// The objects of a comma category that hold rows: a source vertex
        /// with the arrow of the target reaching it (`"id"`, `"src"` or
        /// `"tgt"`), and the edge objects as constraints between two of
        /// them, the pair `(i, j)` the foreign key `e` must hold.
        struct Comma {
            objects: Vec<(String, &'static str)>,
            arrows: Vec<(usize, usize, Edge)>,
        }

        impl Comma {
            fn at(&self, vertex: &str, arrow: &'static str) -> usize {
                self.objects
                    .iter()
                    .position(|(c, p)| c == vertex && *p == arrow)
                    .unwrap()
            }

            /// Add the vertices and internal arrows of `fibre`, reached
            /// along `arrow`.
            fn add_fibre(&mut self, case: &Case, fibre: &[String], arrow: &'static str) {
                for c in fibre {
                    self.objects.push((c.clone(), arrow));
                }
                for e in case
                    .internal
                    .iter()
                    .filter(|e| fibre.contains(&e.src.to_string()))
                {
                    let (i, j) = (self.at(&e.src, arrow), self.at(&e.tgt, arrow));
                    self.arrows.push((i, j, e.clone()));
                }
            }

            fn cones(&self, inst: &FInstance) -> Option<Vec<Vec<RowId>>> {
                let domains: Vec<Vec<RowId>> = self
                    .objects
                    .iter()
                    .map(|(c, _)| inst.rows(c).into_iter().map(|(id, _)| id).collect())
                    .collect();
                let bound = domains
                    .iter()
                    .try_fold(1_usize, |acc, d| acc.checked_mul(d.len().max(1)));
                if bound.is_none_or(|b| b > 50_000) {
                    return None;
                }
                Some(enumerate_cones(inst, &domains, &self.arrows))
            }
        }

        /// A tuple of a vertex: the row drawn from each fibre component.
        type Tuple = Vec<(Name, RowId)>;

        /// The oracle's limit: at each image vertex the tuples of its cones,
        /// and at each target edge between image vertices the pairs of
        /// tuples of its cones. `None` when a naive search space is too
        /// large.
        struct Limit {
            vertices: BTreeMap<String, BTreeSet<Tuple>>,
            edges: BTreeMap<Edge, BTreeSet<(Tuple, Tuple)>>,
        }

        fn oracle(case: &Case, inst: &FInstance) -> Option<Limit> {
            let fibres = fibres(case);
            let tuple = |comma: &Comma, cone: &[RowId], fibre: &[String], arrow| -> Tuple {
                fibre
                    .iter()
                    .map(|c| (Name::from(c.as_str()), cone[comma.at(c, arrow)]))
                    .collect()
            };
            let mut vertices = BTreeMap::new();
            for (d, fibre) in &fibres {
                let mut comma = Comma {
                    objects: Vec::new(),
                    arrows: Vec::new(),
                };
                comma.add_fibre(case, fibre, "id");
                let cones = comma.cones(inst)?;
                let listed: BTreeSet<Tuple> = cones
                    .iter()
                    .map(|cone| tuple(&comma, cone, fibre, "id"))
                    .collect();
                // The fibre tuples tell the cones apart: a foreign key is a
                // set, so a cone has one witness per edge object.
                assert_eq!(listed.len(), cones.len());
                vertices.insert(d.clone(), listed);
            }
            let mut edges = BTreeMap::new();
            for edge in &case.target_edges {
                let (Some(from), Some(to)) = (
                    fibres.get(&edge.src.to_string()),
                    fibres.get(&edge.tgt.to_string()),
                ) else {
                    continue;
                };
                let mut comma = Comma {
                    objects: Vec::new(),
                    arrows: Vec::new(),
                };
                comma.add_fibre(case, from, "src");
                comma.add_fibre(case, to, "tgt");
                for (e, _) in case.carried.iter().filter(|(_, image)| image == edge) {
                    let (i, j) = (comma.at(&e.src, "src"), comma.at(&e.tgt, "tgt"));
                    comma.arrows.push((i, j, e.clone()));
                }
                let cones = comma.cones(inst)?;
                let listed: BTreeSet<(Tuple, Tuple)> = cones
                    .iter()
                    .map(|cone| {
                        (
                            tuple(&comma, cone, from, "src"),
                            tuple(&comma, cone, to, "tgt"),
                        )
                    })
                    .collect();
                assert_eq!(listed.len(), cones.len());
                edges.insert(edge.clone(), listed);
            }
            Some(Limit { vertices, edges })
        }

        /// Pi's result read the oracle's way: the tuples of each vertex
        /// and the pairs of tuples of each target edge between image
        /// vertices.
        fn observed(case: &Case, inst: &FInstance) -> Limit {
            let (pi, projections) = pi_of(case, inst);
            let fibres = fibres(case);
            let tuple_of = |id: RowId| -> Tuple {
                projections
                    .iter()
                    .filter_map(|(c, hom)| hom.row_map.get(&id).map(|&row| (c.clone(), row)))
                    .collect()
            };
            let vertices = fibres
                .keys()
                .map(|d| (d.clone(), tuples(&pi, &projections, d)))
                .collect();
            let edges = case
                .target_edges
                .iter()
                .filter(|e| {
                    fibres.contains_key(&e.src.to_string())
                        && fibres.contains_key(&e.tgt.to_string())
                })
                .map(|e| {
                    let pairs = pi
                        .pairs(e)
                        .iter()
                        .map(|&(x, y)| (tuple_of(x), tuple_of(y)))
                        .collect();
                    (e.clone(), pairs)
                })
                .collect();
            Limit { vertices, edges }
        }

        /// `J`: `I` with more rows and more pairs on its foreign keys.
        fn grown(case: &Case, seed: u64) -> FInstance {
            let mut r = Lcg(seed);
            let mut j = case.inst.clone();
            for c in case.vertex_map.keys() {
                j = j.with_table(c.as_str(), blank(r.below(2))).unwrap();
            }
            let mut edges: Vec<Edge> = case.internal.clone();
            edges.extend(case.carried.iter().map(|(e, _)| e.clone()));
            for e in edges {
                let fresh: Vec<(RowId, RowId)> = random_pairs(&mut r, &j, &e.src, &e.tgt)
                    .into_iter()
                    .filter(|&(s, t)| !j.contains_pair(&e, s, t))
                    .collect();
                j = j.with_foreign_key(e, fresh).unwrap();
            }
            j
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(2048))]

            /// On every case, Pi's rows at each image vertex are the
            /// oracle's cones (compared by the rows they draw from the
            /// fibre), and the pairs of each target edge between image
            /// vertices are the oracle's cones at that edge; a vertex
            /// outside the image has no table.
            #[test]
            fn pi_is_the_limit_over_the_comma_category(seed in any::<u64>()) {
                let case = case(seed);
                let Some(expected) = oracle(&case, &case.inst) else {
                    return Ok(());
                };
                let got = observed(&case, &case.inst);
                prop_assert_eq!(&got.vertices, &expected.vertices);
                prop_assert_eq!(&got.edges, &expected.edges);
                let (pi, _) = pi_of(&case, &case.inst);
                let fibres = fibres(&case);
                prop_assert!(pi.table_names().iter().all(|t| fibres.contains_key(&t.to_string())));
            }

            /// Pi is monotone: adding rows and pairs to the input keeps every
            /// row and every pair of the output, read by the rows it draws.
            #[test]
            fn pi_is_monotone(seed in any::<u64>(), more in any::<u64>()) {
                let case = case(seed);
                let j = grown(&case, more);
                let (before, after) = (observed(&case, &case.inst), observed(&case, &j));
                for (d, rows) in &before.vertices {
                    prop_assert!(rows.is_subset(&after.vertices[d]), "vertex {}", d);
                }
                for (e, pairs) in &before.edges {
                    prop_assert!(pairs.is_subset(&after.edges[e]), "edge {:?}", e);
                }
            }

            /// Along the identity of a schema with cycles and loops, Pi of
            /// any instance is that instance, ids and pairs included, the
            /// foreign keys partial, many-valued or total.
            #[test]
            fn pi_along_the_identity_of_a_cyclic_schema_is_the_identity(seed in any::<u64>()) {
                let mut r = Lcg(seed);
                let vertices = 1 + r.below(3);
                let mut inst = FInstance::new();
                let mut migration = CompiledMigration::default();
                for i in 0..vertices {
                    let name = format!("s{i}");
                    inst = inst.with_table(name.clone(), blank(r.below(4))).unwrap();
                    migration.surviving_verts.insert(name.as_str().into());
                }
                let mut edges = Vec::new();
                for k in 0..r.below(5) {
                    let (a, b) = (format!("s{}", r.below(vertices)), format!("s{}", r.below(vertices)));
                    let edge = fk(&a, &b, &format!("e{k}"));
                    let pairs = random_pairs(&mut r, &inst, &a, &b);
                    inst = inst.with_foreign_key(edge.clone(), pairs).unwrap();
                    migration.surviving_edges.insert(edge.clone());
                    edges.push(edge);
                }
                let schema = make_test_schema(&[], &edges);
                let (pi, projections) =
                    super::functor_pi_with_hom(&inst, &schema, &schema, &migration, 1_000_000).unwrap();
                prop_assert_eq!(&pi, &inst);
                for (_, hom) in projections {
                    prop_assert!(hom.row_map.iter().all(|(a, b)| a == b));
                }
            }
        }
    }

    /// Failing-first checks of the relation-ified reading of tables.
    mod relational {
        use super::*;

        fn blank_rows(ids: &[u32]) -> Vec<(RowId, Row)> {
            ids.iter().map(|&i| (RowId(i), Row::new())).collect()
        }

        #[test]
        fn pi_at_a_target_table_outside_the_image_has_one_row() {
            let src = make_test_schema(&["a"], &[]);
            let tgt = make_test_schema(&["a", "z"], &[]);
            let migration = migration_of(&[("a", "a")], &[]);
            let inst = FInstance::new().with_rows("a", blank_rows(&[0, 1])).unwrap();
            let pi = functor_pi(&inst, &src, &tgt, &migration, 100).unwrap();
            assert_eq!(pi.row_count("a"), 2);
            assert_eq!(pi.row_count("z"), 1, "the limit of the empty diagram is one row");
        }

        #[test]
        fn pi_at_a_target_only_relation_holds_every_pair() {
            let e = fk("a", "z", "e");
            let src = make_test_schema(&["a"], &[]);
            let tgt = make_test_schema(&["a", "z"], &[e.clone()]);
            let migration = migration_of(&[("a", "a")], &[]);
            let inst = FInstance::new().with_rows("a", blank_rows(&[0, 1])).unwrap();
            let pi = functor_pi(&inst, &src, &tgt, &migration, 100).unwrap();
            assert_eq!(pi.pairs(&e).len(), 2, "every pair of a row of a and the row of z");
        }

        #[test]
        fn unmentioned_edges_are_outside_the_domain() {
            let e = fk("a", "b", "e");
            let src = make_test_schema(&["a", "b"], &[e.clone()]);
            let tgt = make_test_schema(&["m"], &[]);
            let migration = migration_of(&[("a", "m"), ("b", "m")], &[]);
            let inst = FInstance::new()
                .with_rows("a", blank_rows(&[0, 1]))
                .and_then(|f| f.with_rows("b", blank_rows(&[2, 3])))
                .and_then(|f| f.with_foreign_key(e, [(RowId(0), RowId(3))]))
                .unwrap();
            let pi = functor_pi(&inst, &src, &tgt, &migration, 100).unwrap();
            assert_eq!(pi.row_count("m"), 4, "an unmentioned edge equalises nothing");
        }

        #[test]
        fn a_limit_row_carries_the_cells_its_components_share() {
            let src = make_test_schema(&["a", "b"], &[]);
            let tgt = make_test_schema(&["m"], &[]);
            let migration = migration_of(&[("a", "m"), ("b", "m")], &[]);
            let x = |v: i64| HashMap::from([("x".to_owned(), Value::Int(v))]);
            let inst = FInstance::new()
                .with_rows("a", [(RowId(0), x(1)), (RowId(1), x(2))])
                .and_then(|f| f.with_rows("b", [(RowId(2), x(1)), (RowId(3), HashMap::new())]))
                .unwrap();
            let (pi, projections) = functor_pi_with_hom(&inst, &src, &tgt, &migration, 100).unwrap();
            assert_eq!(pi.row_count("m"), 1, "only (0, 2) agree on every cell");
            for (id, row) in pi.rows("m") {
                for hom in projections.values() {
                    let component = hom.image(id).unwrap();
                    assert!(crate::functor::same_row(row, inst.row(component).unwrap()));
                }
            }
        }

        #[test]
        fn results_do_not_depend_on_target_names() {
            let src = make_test_schema(&["a", "b", "c", "d"], &[]);
            let run = |left: &str, right: &str| {
                let tgt = make_test_schema(&[left, right], &[]);
                let migration =
                    migration_of(&[("a", left), ("b", left), ("c", right), ("d", right)], &[]);
                let inst = FInstance::new()
                    .with_rows("a", blank_rows(&[0]))
                    .and_then(|f| f.with_rows("b", blank_rows(&[1])))
                    .and_then(|f| f.with_rows("c", blank_rows(&[2])))
                    .and_then(|f| f.with_rows("d", blank_rows(&[3])))
                    .unwrap();
                functor_pi(&inst, &src, &tgt, &migration, 100).unwrap()
            };
            let before = run("m", "n");
            let after = run("q", "p");
            assert_eq!(before.rows("m").first().map(|r| r.0), after.rows("q").first().map(|r| r.0));
            assert_eq!(before.rows("n").first().map(|r| r.0), after.rows("p").first().map(|r| r.0));
        }
    }
}
