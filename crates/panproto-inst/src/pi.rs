//! Right Kan extension (`Pi_F`) for instances.
//!
//! For set-valued functor instances, `Pi_F` is the right Kan extension of
//! presheaves on relation-ified schemas ([`crate::RelSchema`]) along the
//! functor a migration states ([`MigrationFunctor`]): at each target object
//! it is the limit of the instance over the comma category under that
//! object. For W-type instances, only vertex-injective migrations are
//! supported; a non-injective map is rejected rather than silently
//! producing `Sigma`-shaped output mislabeled as `Pi`.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use panproto_gat::Name;
use panproto_schema::{CanonicalWriter, Edge, Schema};

use crate::error::RestrictError;
use crate::functor::{FInstance, Row, allocate_row, row_allocator};
use crate::identity::write_value_map;
use crate::instance_hom::FInstanceHom;
use crate::metadata::Node;
use crate::migration_functor::MigrationFunctor;
use crate::view_ref::RowId;
use crate::wtype::{CompiledMigration, WInstance, reconstruct_fans, resolve_edge};

/// Right Kan extension (`Pi_F`) of a table instance along the functor
/// `migration` states from `src_schema` to `tgt_schema`.
///
/// Builds the [`MigrationFunctor`] and takes [`functor_pi_along`].
///
/// # Errors
///
/// As [`MigrationFunctor::new`] and [`functor_pi_along`].
pub fn functor_pi(
    instance: &FInstance,
    src_schema: &Schema,
    tgt_schema: &Schema,
    migration: &CompiledMigration,
    max_product_size: usize,
) -> Result<FInstance, RestrictError> {
    let functor = MigrationFunctor::new(src_schema, tgt_schema, migration)?;
    functor_pi_along(instance, &functor, max_product_size)
}

/// [`functor_pi`] with the projection of every row of the result onto each
/// fibre component, as [`functor_pi_along_with_hom`] returns them.
///
/// # Errors
///
/// As [`functor_pi`].
pub fn functor_pi_with_hom(
    instance: &FInstance,
    src_schema: &Schema,
    tgt_schema: &Schema,
    migration: &CompiledMigration,
    max_product_size: usize,
) -> Result<(FInstance, BTreeMap<Name, FInstanceHom>), RestrictError> {
    let functor = MigrationFunctor::new(src_schema, tgt_schema, migration)?;
    functor_pi_along_with_hom(instance, &functor, max_product_size)
}

/// Right Kan extension (`Pi_F`) of a table instance along `functor`.
///
/// The first projection of [`functor_pi_along_with_hom`]; see there.
///
/// # Errors
///
/// As [`functor_pi_along_with_hom`].
pub fn functor_pi_along(
    instance: &FInstance,
    functor: &MigrationFunctor,
    max_product_size: usize,
) -> Result<FInstance, RestrictError> {
    pi_limit(instance, functor, max_product_size).map(|limit| limit.instance)
}

/// Right Kan extension (`Pi_F`) of a table instance along `functor`, with
/// the projection of every row of the result onto each fibre component.
///
/// The instance is read as a presheaf `I` on the kept part `S'` of the source
/// relation-ified schema: its rows at tables of `S'` and its pairs at
/// relations of `S'`; rows and pairs elsewhere are outside the domain of
/// `F` and are not read. `Pi_F(I)` at a target object `d` is the limit of `I`
/// over the comma category `d/F`, computed by the standard formula:
///
/// - At a target table `d`, the only arrow out of `d` is its identity, so the
///   objects of `d/F` are the tables of the fibre `F^{-1}(d)` and it has no
///   other arrows. The rows of `d` are the tuples with one row of each fibre
///   table (fibre tables ascending). Cells are labels every homomorphism
///   preserves exactly (attribute vertices are not objects), so a tuple is
///   a cone only when its rows carry the same cells, and the row takes them.
///   A table outside the image of `F` has the empty diagram, whose limit is
///   one row; it carries no cells.
/// - At a target relation `E: d -> d'`, the objects of `E/F` are the
///   relations of `F^{-1}(E)` together with the fibre tables of `d` (reached
///   along `src_E`) and of `d'` (along `tgt_E`). The pairs of `E` are the
///   pairs `(x, y)` of a row of `d` and a row of `d'` such that every
///   relation `e` sent to `E` holds the pair of `x`'s row at `e`'s source and
///   `y`'s row at `e`'s target, so several relations sent to `E` take the
///   intersection. With none sent to `E` every pair of rows is in the limit.
///
/// Every comma category is finite, so every limit is. Along the identity
/// functor `Pi_F` is the identity, and it is monotone in `I`.
///
/// Row ids: the rows of a table whose fibre is one table keep the ids of the
/// rows they project to. Every other row is fresh, allocated from the least
/// id above every row of `instance`: first the tables in the image, ordered
/// by the least table of their fibre, each table's tuples in ascending
/// order of their rows, then the tables outside the image, ascending. So the
/// result does not depend on the names of the target tables in the image;
/// renaming the tables outside the image can only permute the ids of their
/// one row each.
///
/// The returned map sends each table `s` of `S'` to the projection of the
/// rows of `F(s)` onto their row at `s`, as an [`FInstanceHom`] whose table
/// map sends `F(s)` to `s`.
///
/// # Errors
///
/// Returns [`RestrictError::ProductSizeExceeded`] when a target table would
/// hold more than `max_product_size` tuples, or a relation to which no
/// relation is sent more than `max_product_size` pairs (reported at its
/// source table), and [`RestrictError::RowIdsExhausted`] when fresh row ids
/// run out. Tables are checked in ascending name, then relations in
/// ascending order.
pub fn functor_pi_along_with_hom(
    instance: &FInstance,
    functor: &MigrationFunctor,
    max_product_size: usize,
) -> Result<(FInstance, BTreeMap<Name, FInstanceHom>), RestrictError> {
    let limit = pi_limit(instance, functor, max_product_size)?;
    let mut projections: BTreeMap<Name, FInstanceHom> = BTreeMap::new();
    for (&row, (table, tuple)) in &limit.tuples {
        for (component, &source_row) in functor.table_fibre(table).into_iter().zip(tuple) {
            projections
                .entry(component.clone())
                .or_insert_with(|| {
                    FInstanceHom::default()
                        .with_table_map(BTreeMap::from([(table.clone(), component.clone())]))
                })
                .row_map
                .insert(row, source_row);
        }
    }
    Ok((limit.instance, projections))
}

/// The limit `Pi_F(I)` together with the tuple behind each of its rows.
pub(crate) struct PiLimit {
    /// The instance `Pi_F(I)`.
    pub(crate) instance: FInstance,
    /// Each row's target table and tuple: one row of each fibre table, the
    /// fibre ascending (empty at a table outside the image).
    pub(crate) tuples: BTreeMap<RowId, (Name, Vec<RowId>)>,
    /// The row of each target table and tuple.
    pub(crate) rows: HashMap<(Name, Vec<RowId>), RowId>,
}

/// The canonical bytes of a row's cells: equal exactly when the cells are
/// equal as every homomorphism compares them.
fn cell_key(row: &Row) -> Vec<u8> {
    let mut writer = CanonicalWriter::new();
    write_value_map(&mut writer, row);
    writer.into_bytes()
}

/// The tuples of one target table with their cells.
struct TableLimit<'a> {
    /// The target table.
    table: &'a Name,
    /// Its fibre, ascending.
    fibre: Vec<&'a Name>,
    /// The tuples, ascending, each with its cells.
    tuples: Vec<(Vec<RowId>, Row)>,
}

/// The cones at one target table: tuples with one row of each fibre table,
/// all carrying the same cells, ascending.
fn table_limit<'a>(
    instance: &FInstance,
    table: &'a Name,
    fibre: Vec<&'a Name>,
    max: usize,
) -> Result<TableLimit<'a>, RestrictError> {
    let Some((first, rest)) = fibre.split_first() else {
        return Ok(TableLimit {
            table,
            fibre,
            tuples: vec![(Vec::new(), Row::new())],
        });
    };
    // The rows of every later component, grouped by their cells.
    let groups: Vec<HashMap<Vec<u8>, Vec<RowId>>> = rest
        .iter()
        .map(|component| {
            let mut by_cells: HashMap<Vec<u8>, Vec<RowId>> = HashMap::new();
            for (id, row) in instance.rows(component) {
                by_cells.entry(cell_key(row)).or_default().push(id);
            }
            by_cells
        })
        .collect();
    let mut tuples = Vec::new();
    for (id, row) in instance.rows(first) {
        let key = cell_key(row);
        let choices: Vec<&[RowId]> = groups
            .iter()
            .map(|group| group.get(&key).map_or(&[][..], Vec::as_slice))
            .collect();
        let count = choices
            .iter()
            .try_fold(1usize, |acc, c| acc.checked_mul(c.len()))
            .unwrap_or(usize::MAX);
        if count == 0 {
            continue;
        }
        if tuples.len().saturating_add(count) > max {
            return Err(RestrictError::ProductSizeExceeded {
                vertex: table.to_string(),
                actual: tuples.len().saturating_add(count),
                limit: max,
            });
        }
        // Every choice of one row per later component, the last varying
        // fastest, so the tuples come out ascending.
        let mut index = vec![0usize; choices.len()];
        loop {
            let mut tuple = Vec::with_capacity(fibre.len());
            tuple.push(id);
            tuple.extend(choices.iter().zip(&index).map(|(c, &i)| c[i]));
            tuples.push((tuple, row.clone()));
            let mut position = choices.len();
            loop {
                if position == 0 {
                    break;
                }
                position -= 1;
                index[position] += 1;
                if index[position] < choices[position].len() {
                    break;
                }
                index[position] = 0;
            }
            if index.iter().all(|&i| i == 0) {
                break;
            }
        }
    }
    Ok(TableLimit {
        table,
        fibre,
        tuples,
    })
}

/// The pairs of the limit at one target relation, as pairs of tuple
/// positions in the limits at its endpoint tables, ascending.
fn relation_limit(
    instance: &FInstance,
    relation: &Edge,
    carried: &[&Edge],
    from: &TableLimit<'_>,
    to: &TableLimit<'_>,
    max: usize,
) -> Result<Vec<(usize, usize)>, RestrictError> {
    let Some((first, rest)) = carried.split_first() else {
        let count = from.tuples.len().saturating_mul(to.tuples.len());
        if count > max {
            return Err(RestrictError::ProductSizeExceeded {
                vertex: relation.src.to_string(),
                actual: count,
                limit: max,
            });
        }
        return Ok((0..from.tuples.len())
            .flat_map(|x| (0..to.tuples.len()).map(move |y| (x, y)))
            .collect());
    };
    // The position of a relation's endpoint tables among the fibres.
    let at = |limit: &TableLimit<'_>, table: &Name| {
        limit.fibre.iter().position(|component| *component == table)
    };
    let (Some(first_src), Some(first_tgt)) = (at(from, &first.src), at(to, &first.tgt)) else {
        return Ok(Vec::new());
    };
    let index = |limit: &TableLimit<'_>, position: usize| {
        let mut by_row: HashMap<RowId, Vec<usize>> = HashMap::new();
        for (i, (tuple, _)) in limit.tuples.iter().enumerate() {
            by_row.entry(tuple[position]).or_default().push(i);
        }
        by_row
    };
    let (sources, targets) = (index(from, first_src), index(to, first_tgt));
    let others: Vec<(&Edge, usize, usize)> = rest
        .iter()
        .filter_map(|edge| Some((*edge, at(from, &edge.src)?, at(to, &edge.tgt)?)))
        .collect();
    let mut pairs = BTreeSet::new();
    for &(s, t) in instance.pairs(first) {
        let (Some(xs), Some(ys)) = (sources.get(&s), targets.get(&t)) else {
            continue;
        };
        for &x in xs {
            for &y in ys {
                let (tx, ty) = (&from.tuples[x].0, &to.tuples[y].0);
                if others
                    .iter()
                    .all(|(edge, ps, pt)| instance.contains_pair(edge, tx[*ps], ty[*pt]))
                {
                    pairs.insert((x, y));
                }
            }
        }
    }
    Ok(pairs.into_iter().collect())
}

/// `Pi_F(I)` with the tuple behind each row; see
/// [`functor_pi_along_with_hom`].
pub(crate) fn pi_limit(
    instance: &FInstance,
    functor: &MigrationFunctor,
    max: usize,
) -> Result<PiLimit, RestrictError> {
    let mut limits: BTreeMap<&Name, TableLimit<'_>> = BTreeMap::new();
    for (table, fibre) in functor.table_fibres() {
        limits.insert(table, table_limit(instance, table, fibre, max)?);
    }
    let mut relation_limits: Vec<(&Edge, Vec<(usize, usize)>)> = Vec::new();
    for (relation, carried) in functor.relation_fibres() {
        let pairs = relation_limit(
            instance,
            relation,
            &carried,
            &limits[&relation.src],
            &limits[&relation.tgt],
            max,
        )?;
        relation_limits.push((relation, pairs));
    }

    // Image tables by their least fibre table, then the tables outside the
    // image by name.
    let mut order: Vec<&TableLimit<'_>> = limits.values().collect();
    order.sort_by(|a, b| match (a.fibre.first(), b.fibre.first()) {
        (Some(x), Some(y)) => x.cmp(y),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.table.cmp(b.table),
    });
    let mut allocator = row_allocator(instance.row_ids());
    let mut out = FInstance::new();
    let mut tuples = BTreeMap::new();
    let mut rows = HashMap::new();
    let mut ids: HashMap<&Name, Vec<RowId>> = HashMap::new();
    for limit in order {
        let mut placed = Vec::with_capacity(limit.tuples.len());
        for (tuple, cells) in &limit.tuples {
            let id = if limit.fibre.len() == 1 {
                tuple[0]
            } else {
                allocate_row(&mut allocator).map_err(|_| RestrictError::RowIdsExhausted)?
            };
            out.place_row(limit.table.clone(), id, cells.clone());
            tuples.insert(id, (limit.table.clone(), tuple.clone()));
            rows.insert((limit.table.clone(), tuple.clone()), id);
            placed.push(id);
        }
        ids.insert(limit.table, placed);
    }
    for (relation, pairs) in relation_limits {
        let (from, to) = (&ids[&relation.src], &ids[&relation.tgt]);
        for (x, y) in pairs {
            out.place_pair(relation.clone(), from[x], to[y]);
        }
    }
    Ok(PiLimit {
        instance: out,
        tuples,
        rows,
    })
}

/// The fibres of a compiled migration's vertex map, for the W-type
/// extension: each target vertex with the source vertices sent to it, both
/// ascending.
fn wtype_fibres(migration: &CompiledMigration) -> BTreeMap<Name, Vec<Name>> {
    let mut fibres: BTreeMap<Name, Vec<Name>> = BTreeMap::new();
    for source in crate::adjunction::source_vertices(migration) {
        let target = crate::adjunction::map_vertex(migration, &source);
        fibres.entry(target).or_default().push(source);
    }
    fibres
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
    let fiber_map = wtype_fibres(migration);

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
    use std::collections::{HashMap, HashSet};

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

    // --- functor_pi along a migration functor ---

    /// A foreign key `src -> tgt` named `name`.
    fn fk(src: &str, tgt: &str, name: &str) -> Edge {
        Edge {
            src: src.into(),
            tgt: tgt.into(),
            kind: "fk".into(),
            name: Some(name.into()),
        }
    }

    /// Rows without cells, with the given ids.
    fn blank_rows(ids: &[u32]) -> Vec<(RowId, Row)> {
        ids.iter().map(|&i| (RowId(i), Row::new())).collect()
    }

    /// A migration sending each `(source, target)` table and nothing else.
    fn migration_of(tables: &[(&str, &str)]) -> CompiledMigration {
        let mut migration = CompiledMigration::default();
        for &(src, tgt) in tables {
            migration.vertex_remap.insert(src.into(), tgt.into());
            migration.surviving_verts.insert(tgt.into());
        }
        migration
    }

    #[test]
    fn pi_at_a_target_table_outside_the_image_has_one_row() {
        let src = make_test_schema(&["a"], &[]);
        let tgt = make_test_schema(&["a", "z"], &[]);
        let migration = migration_of(&[("a", "a")]);
        let inst = FInstance::new()
            .with_rows("a", blank_rows(&[0, 1]))
            .unwrap();
        let pi = functor_pi(&inst, &src, &tgt, &migration, 100).unwrap();
        assert_eq!(pi.row_count("a"), 2);
        assert_eq!(
            pi.row_count("z"),
            1,
            "the limit of the empty diagram is one row"
        );
        let (_, row) = pi.rows("z")[0];
        assert!(row.is_empty(), "the one row carries no cells");
        assert_eq!(pi.rows("z")[0].0, RowId(2), "a fresh id above the input");
    }

    /// A target array column's element vertex is part of a cell, not a table
    /// outside the image, so the limit gives it no row.
    #[test]
    fn pi_gives_no_row_to_a_part_of_a_cell() {
        let column = Edge {
            src: "post".into(),
            tgt: "post.tags".into(),
            kind: "prop".into(),
            name: Some("tags".into()),
        };
        let items = Edge {
            src: "post.tags".into(),
            tgt: "post.tags:items".into(),
            kind: "items".into(),
            name: None,
        };
        let src = make_test_schema(&["post"], &[]);
        let tgt = make_test_schema(&["post", "post.tags", "post.tags:items"], &[column, items]);
        let migration = migration_of(&[("post", "post")]);
        let inst = FInstance::new()
            .with_rows("post", blank_rows(&[0]))
            .unwrap();
        let pi = functor_pi(&inst, &src, &tgt, &migration, 100).unwrap();
        assert_eq!(pi, inst, "{pi:?}");
    }

    #[test]
    fn pi_at_a_target_only_relation_holds_every_pair() {
        let e = fk("a", "z", "e");
        let src = make_test_schema(&["a"], &[]);
        let tgt = make_test_schema(&["a", "z"], std::slice::from_ref(&e));
        let migration = migration_of(&[("a", "a")]);
        let inst = FInstance::new()
            .with_rows("a", blank_rows(&[0, 1]))
            .unwrap();
        let pi = functor_pi(&inst, &src, &tgt, &migration, 100).unwrap();
        let z = pi.rows("z")[0].0;
        assert!(
            pi.pairs(&e).iter().eq(&[(RowId(0), z), (RowId(1), z)]),
            "every pair of a row of a and the row of z: {:?}",
            pi.pairs(&e)
        );
        assert!(matches!(
            functor_pi(&inst, &src, &tgt, &migration, 1),
            Err(RestrictError::ProductSizeExceeded {
                actual: 2,
                limit: 1,
                ..
            })
        ));
    }

    #[test]
    fn unmentioned_edges_are_outside_the_domain() {
        let e = fk("a", "b", "e");
        let src = make_test_schema(&["a", "b"], std::slice::from_ref(&e));
        let tgt = make_test_schema(&["m"], &[]);
        let migration = migration_of(&[("a", "m"), ("b", "m")]);
        let inst = FInstance::new()
            .with_rows("a", blank_rows(&[0, 1]))
            .and_then(|f| f.with_rows("b", blank_rows(&[2, 3])))
            .and_then(|f| f.with_foreign_key(e, [(RowId(0), RowId(3))]))
            .unwrap();
        let pi = functor_pi(&inst, &src, &tgt, &migration, 100).unwrap();
        assert_eq!(
            pi.row_count("m"),
            4,
            "an unmentioned edge equalises nothing"
        );
        assert_eq!(pi.pair_count(), 0);
    }

    #[test]
    fn a_limit_row_carries_the_cells_its_components_share() {
        let src = make_test_schema(&["a", "b"], &[]);
        let tgt = make_test_schema(&["m"], &[]);
        let migration = migration_of(&[("a", "m"), ("b", "m")]);
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
            let migration = migration_of(&[("a", left), ("b", left), ("c", right), ("d", right)]);
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
        assert_eq!(
            before.rows("m").first().map(|r| r.0),
            after.rows("q").first().map(|r| r.0)
        );
        assert_eq!(
            before.rows("n").first().map(|r| r.0),
            after.rows("p").first().map(|r| r.0)
        );
    }

    /// Two two-row tables merged into `m`, with the foreign key
    /// `l: a -> b = {(a0, b1)}` carried to the loop `L` on `m`.
    fn carried_loop() -> (FInstance, Schema, Schema, CompiledMigration, Edge) {
        let l = fk("a", "b", "l");
        let loop_edge = fk("m", "m", "L");
        let inst = FInstance::new()
            .with_rows("a", blank_rows(&[0, 1]))
            .and_then(|f| f.with_rows("b", blank_rows(&[2, 3])))
            .and_then(|f| f.with_foreign_key(l.clone(), [(RowId(0), RowId(3))]))
            .unwrap();
        let src = make_test_schema(&["a", "b"], std::slice::from_ref(&l));
        let tgt = make_test_schema(&["m"], std::slice::from_ref(&loop_edge));
        let mut migration = migration_of(&[("a", "m"), ("b", "m")]);
        migration.edge_remap.insert(l, loop_edge.clone());
        (inst, src, tgt, migration, loop_edge)
    }

    /// Kept and carried to a loop, the intra-fibre foreign key is
    /// equalised by the limit at the loop: its pairs are exactly the pairs
    /// of tuples `(x, y)` whose `a` row and `b` row the key relates, and the
    /// table of `m` is the whole product (no arrow of the fibre constrains
    /// it). Every cone over `L/F` is one of these pairs and no other.
    #[test]
    fn functor_pi_equalises_along_an_intra_fibre_foreign_key() {
        let (inst, src, tgt, migration, loop_edge) = carried_loop();
        let (pi, projections) = functor_pi_with_hom(&inst, &src, &tgt, &migration, 100).unwrap();
        assert_eq!(pi.row_count("m"), 4);
        let a = &projections[&Name::from("a")];
        let b = &projections[&Name::from("b")];
        let rows: Vec<RowId> = pi.rows("m").into_iter().map(|(id, _)| id).collect();
        let mut cones = BTreeSet::new();
        for &x in &rows {
            for &y in &rows {
                if inst.contains_pair(&fk("a", "b", "l"), a.row_map[&x], b.row_map[&y]) {
                    cones.insert((x, y));
                }
            }
        }
        assert_eq!(
            cones.len(),
            4,
            "x draws a0 (two tuples), y draws b1 (two tuples)"
        );
        assert_eq!(
            pi.pairs(&loop_edge),
            &cones,
            "the loop holds every cone and nothing else"
        );
    }

    /// The interim rejection of fibres with an internal edge is discharged:
    /// over such a fibre Pi returns the limit. With the edge carried to a
    /// loop it is the equaliser at the loop above; left unmentioned the
    /// edge is outside the functor's domain and constrains nothing, so the
    /// table is the bare product and no pair is invented.
    #[test]
    fn pi_dispatch_rejects_a_fibre_with_an_internal_edge() {
        let (inst, src, tgt, mut migration, loop_edge) = carried_loop();
        migration.edge_remap.clear();
        let pi = functor_pi(&inst, &src, &tgt, &migration, 100).unwrap();
        assert_eq!(pi.row_count("m"), 4);
        assert_eq!(
            pi.pairs(&loop_edge).len(),
            16,
            "a target loop nothing is sent to holds every pair of the four rows"
        );
    }

    /// Along the identity migration as `panproto_mig::compile` states it
    /// (every table sent to itself, every foreign key listed as surviving),
    /// Pi returns its input: a post with no author and a post with two keep
    /// their rows, and an org chart whose root has no manager keeps every
    /// employee.
    #[test]
    fn pi_along_a_compiled_identity_keeps_partial_and_many_valued_keys() {
        let author = fk("post", "user", "author");
        let posts = FInstance::new()
            .with_rows("post", blank_rows(&[2, 3, 4]))
            .and_then(|f| f.with_rows("user", blank_rows(&[0, 1])))
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
        let manager = fk("emp", "emp", "manager");
        let chart = FInstance::new()
            .with_rows("emp", blank_rows(&[0, 1, 2, 3]))
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
        for (inst, tables, key) in [
            (posts, &["post", "user"][..], author),
            (chart, &["emp"][..], manager),
        ] {
            let schema = make_test_schema(tables, std::slice::from_ref(&key));
            let mut migration = migration_of(&tables.iter().map(|&t| (t, t)).collect::<Vec<_>>());
            migration.surviving_edges.insert(key);
            let (pi, projections) =
                functor_pi_with_hom(&inst, &schema, &schema, &migration, 100).unwrap();
            assert_eq!(pi, inst);
            for hom in projections.values() {
                assert!(hom.row_map.iter().all(|(row, image)| row == image));
            }
        }
    }

    #[test]
    fn carrying_foreign_keys_through_pi_is_linear() {
        const ROWS: u32 = 20_000;
        let edge = fk("a", "b", "e");
        let cells = |n: u32| HashMap::from([("n".to_owned(), Value::Int(i64::from(n)))]);
        let inst = FInstance::new()
            .with_table("a", (0..ROWS).map(cells))
            .and_then(|f| f.with_table("b", (0..ROWS).map(cells)))
            .and_then(|f| {
                f.with_foreign_key(edge.clone(), (0..ROWS).map(|i| (RowId(i), RowId(ROWS + i))))
            })
            .unwrap();
        let schema = make_test_schema(&["a", "b"], std::slice::from_ref(&edge));
        let migration = CompiledMigration {
            surviving_verts: HashSet::from([Name::from("a"), Name::from("b")]),
            surviving_edges: HashSet::from([edge]),
            ..CompiledMigration::default()
        };
        let began = std::time::Instant::now();
        let pi = functor_pi(&inst, &schema, &schema, &migration, usize::MAX).unwrap();
        let took = began.elapsed();
        assert_eq!(pi, inst, "Pi along the identity is the identity");
        assert!(
            took < std::time::Duration::from_secs(3),
            "Pi over {ROWS} pairs took {took:?}"
        );
    }
}
