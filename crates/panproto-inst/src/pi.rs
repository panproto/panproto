//! Right Kan extension (`Pi_F`) for instances.
//!
//! For table instances, `Pi_F` is the right Kan extension along the functor
//! a migration states between attributed relation-ified schemas
//! ([`MigrationFunctor`]): at each target table the natural join of its
//! fibre's tables on the columns they share in the target, and at each
//! target relation the limit over the relations carried to it. It exists
//! exactly along a functor that is surjective on attributes
//! ([`MigrationFunctor::attribute_surjective`]); along any other it is
//! refused. For W-type instances, only vertex-injective migrations are
//! supported; a non-injective map is rejected rather than silently
//! producing `Sigma`-shaped output mislabeled as `Pi`.

use std::collections::{BTreeMap, HashMap};

use panproto_gat::Name;
use panproto_schema::{Edge, Protocol, Schema};

use crate::adjunction::AdjunctionError;
use crate::error::RestrictError;
use crate::functor::{FInstance, Row, allocate_row, row_allocator};
use crate::metadata::Node;
use crate::migration_functor::MigrationFunctor;
use crate::value::Value;
use crate::view_ref::RowId;
use crate::wtype::{CompiledMigration, WInstance, reconstruct_fans, resolve_edge};

/// Where the rows of a `Pi_F` result come from.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PiProvenance {
    /// Each row of the result with its target table and its tuple: one row
    /// of each table of the table's fibre, the fibre ascending (empty at a
    /// table outside the image).
    pub tuples: BTreeMap<RowId, (Name, Vec<RowId>)>,
}

/// Right Kan extension (`Pi_F`) of a table instance along the functor
/// `migration` states from `src_schema` (under `src_protocol`) to
/// `tgt_schema` (under `tgt_protocol`).
///
/// Builds the [`MigrationFunctor`] and takes [`functor_pi_along`].
///
/// # Errors
///
/// [`AdjunctionError::Pi`] wrapping the error of [`MigrationFunctor::new`],
/// and as [`functor_pi_along`].
pub fn functor_pi(
    instance: &FInstance,
    src_schema: &Schema,
    src_protocol: &Protocol,
    tgt_schema: &Schema,
    tgt_protocol: &Protocol,
    migration: &CompiledMigration,
    max_product_size: usize,
) -> Result<FInstance, AdjunctionError> {
    let functor = MigrationFunctor::new(
        src_schema,
        src_protocol,
        tgt_schema,
        tgt_protocol,
        migration,
    )
    .map_err(AdjunctionError::Pi)?;
    functor_pi_along(instance, &functor, max_product_size)
}

/// Right Kan extension (`Pi_F`) of a table instance along `functor`.
///
/// The first projection of [`functor_pi_along_with_provenance`]; see there.
///
/// # Errors
///
/// As [`functor_pi_along_with_provenance`].
pub fn functor_pi_along(
    instance: &FInstance,
    functor: &MigrationFunctor,
    max_product_size: usize,
) -> Result<FInstance, AdjunctionError> {
    functor_pi_along_with_provenance(instance, functor, max_product_size).map(|(pi, _)| pi)
}

/// Right Kan extension (`Pi_F`) of a table instance along `functor`, with
/// the tuple behind each row of the result.
///
/// `functor` must be surjective on attributes. The instance must conform to
/// the whole source schema `S`; it is then restricted to the kept part `S'`,
/// `I = rho_{S'}(x)`. `Pi_F(I)` at a target object `d` is the limit of `I`
/// over the comma category under `d`:
///
/// - At a target table `d` with fibre `s_1 < ... < s_k`, a row is a tuple
///   `(r_1, ..., r_k)` of one row of each fibre table that is a cone: for
///   every column `beta` of `d`, the values of the `r_i` at every column
///   `alpha` of `s_i` with `F(alpha) = beta` are equal (equal canonical
///   bytes; a labelled null equals only itself), and that value is the row's
///   cell at `beta`. This is the natural join of the fibre's tables on the
///   columns they share in the target, and the product of their rows where
///   they share none. A table outside the image has, by surjectivity, no
///   columns, and gets the one row of the empty diagram's limit.
/// - At a target relation `E: d -> d'`, the pairs are the pairs `(x, y)` of a
///   row of `d` and a row of `d'` such that every relation `e` carried to `E`
///   holds the pair of `x`'s row at `e`'s source and `y`'s row at `e`'s
///   target (several relations carried to `E` take the intersection). With
///   none carried, every pair of rows is in the limit.
///
/// `Pi_F` creates no labelled nulls. Along the identity functor it is the
/// identity, and it is monotone in `I`.
///
/// Row ids: the rows of a table whose fibre is one table keep the ids of the
/// rows they project to. Every other row is fresh, allocated from the least
/// id above every row of `instance`: first the tables in the image, ordered
/// by the least table of their fibre, each table's tuples in ascending
/// order, then the tables outside the image, ascending. So the result does
/// not depend on the names of the target tables in the image; renaming the
/// tables outside the image can only permute the ids of their one row each.
///
/// # Errors
///
/// [`AdjunctionError::Pi`] wrapping
/// [`RestrictError::AttributeOutsideImage`] when `functor` is not surjective
/// on attributes (checked first); [`AdjunctionError::NonConforming`] when the
/// instance does not conform to `S`; [`AdjunctionError::Pi`] wrapping
/// [`RestrictError::ProductSizeExceeded`] when a target table would hold
/// more than `max_product_size` rows, or a relation to which no relation is
/// carried more than `max_product_size` pairs (reported at its source table;
/// tables are checked in ascending name, then relations in ascending order),
/// and [`RestrictError::RowIdsExhausted`] when fresh row ids run out.
pub fn functor_pi_along_with_provenance(
    instance: &FInstance,
    functor: &MigrationFunctor,
    max_product_size: usize,
) -> Result<(FInstance, PiProvenance), AdjunctionError> {
    functor
        .attribute_surjective()
        .map_err(AdjunctionError::Pi)?;
    let restricted = crate::adjunction::f_restrict_to_domain(instance, functor)?;
    let limit = pi_limit(&restricted, functor, max_product_size, instance.row_ids())
        .map_err(AdjunctionError::Pi)?;
    Ok((
        limit.instance,
        PiProvenance {
            tuples: limit.tuples,
        },
    ))
}

/// `Pi_F` of `i`, an instance on the kept part `S'` of the source.
///
/// `i` is in the form [`crate::f_restrict_to_domain`] gives, and fresh row
/// ids are allocated above `i`'s. This is the `Pi_F` the units, counits,
/// transposes and [`crate::f_pi_on_hom`] use.
///
/// # Errors
///
/// [`AdjunctionError::NonConforming`] when `i` does not conform to `S'`, and
/// as [`functor_pi_along_with_provenance`].
pub fn functor_pi_restricted(
    i: &FInstance,
    functor: &MigrationFunctor,
    max_product_size: usize,
) -> Result<(FInstance, PiProvenance), AdjunctionError> {
    let limit = crate::adjunction::pi_on_domain(i, functor, max_product_size)?;
    Ok((
        limit.instance,
        PiProvenance {
            tuples: limit.tuples,
        },
    ))
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

/// A row read at the target columns: each column's value with its canonical
/// bytes.
type AtTarget = BTreeMap<String, (Vec<u8>, Value)>;

/// The tuples of one target table with their cells.
struct TableLimit<'a> {
    /// The target table.
    table: &'a Name,
    /// Its fibre, ascending.
    fibre: Vec<&'a Name>,
    /// The tuples, ascending, each with its cells.
    tuples: Vec<(Vec<RowId>, Row)>,
}

/// One row of a fibre table read at the target columns: the value at each
/// column its kept columns are sent to, or `None` when two of its own
/// columns sent to one target column disagree (it is then in no cone).
fn row_at_target(functor: &MigrationFunctor, component: &Name, cells: &Row) -> Option<AtTarget> {
    let mut at: AtTarget = BTreeMap::new();
    for alpha in functor.kept_columns(component) {
        let beta = functor.column_image(component, alpha)?;
        let value = cells.get(alpha)?;
        let bytes = value.canonical_bytes();
        match at.get(beta) {
            Some((seen, _)) if *seen != bytes => return None,
            Some(_) => {}
            None => {
                at.insert(beta.to_owned(), (bytes, value.clone()));
            }
        }
    }
    Some(at)
}

/// The cones at one target table: tuples with one row of each fibre table
/// whose values agree at every target column, ascending, joined component
/// by component on the target columns already assigned.
fn table_limit<'a>(
    instance: &FInstance,
    functor: &MigrationFunctor,
    table: &'a Name,
    fibre: Vec<&'a Name>,
    max: usize,
) -> Result<TableLimit<'a>, RestrictError> {
    type Partial = (Vec<RowId>, AtTarget);
    let mut partials: Vec<Partial> = vec![(Vec::new(), BTreeMap::new())];
    let last = fibre.len().saturating_sub(1);
    for (position, component) in fibre.iter().enumerate() {
        let rows: Vec<(RowId, AtTarget)> = instance
            .rows(component)
            .into_iter()
            .filter_map(|(id, cells)| Some((id, row_at_target(functor, component, cells)?)))
            .collect();
        // The target columns this component shares with those assigned so
        // far, which are the same for every partial tuple.
        let assigned: Vec<String> = partials
            .first()
            .map(|(_, at)| at.keys().cloned().collect())
            .unwrap_or_default();
        let own: Vec<String> = functor
            .kept_columns(component)
            .into_iter()
            .filter_map(|alpha| functor.column_image(component, alpha))
            .map(str::to_owned)
            .collect();
        let shared: Vec<&String> = assigned.iter().filter(|c| own.contains(c)).collect();
        let key = |at: &AtTarget| -> Option<Vec<Vec<u8>>> {
            shared
                .iter()
                .map(|c| at.get(*c).map(|(bytes, _)| bytes.clone()))
                .collect()
        };
        let mut index: HashMap<Vec<Vec<u8>>, Vec<usize>> = HashMap::new();
        for (n, (_, at)) in rows.iter().enumerate() {
            if let Some(k) = key(at) {
                index.entry(k).or_default().push(n);
            }
        }
        if position == last {
            let count = partials.iter().try_fold(0usize, |acc, (_, at)| {
                let matches = key(at).and_then(|k| index.get(&k)).map_or(0, Vec::len);
                acc.checked_add(matches)
            });
            let count = count.unwrap_or(usize::MAX);
            if count > max {
                return Err(RestrictError::ProductSizeExceeded {
                    vertex: table.to_string(),
                    actual: count,
                    limit: max,
                });
            }
        }
        let mut next: Vec<Partial> = Vec::new();
        for (tuple, at) in &partials {
            let Some(matches) = key(at).and_then(|k| index.get(&k)) else {
                continue;
            };
            for &n in matches {
                let (id, row_at) = &rows[n];
                let mut tuple = tuple.clone();
                tuple.push(*id);
                let mut joined = at.clone();
                joined.extend(row_at.iter().map(|(c, v)| (c.clone(), v.clone())));
                next.push((tuple, joined));
            }
        }
        partials = next;
    }
    if fibre.is_empty() && partials.len() > max {
        return Err(RestrictError::ProductSizeExceeded {
            vertex: table.to_string(),
            actual: partials.len(),
            limit: max,
        });
    }
    let mut tuples: Vec<(Vec<RowId>, Row)> = partials
        .into_iter()
        .map(|(tuple, at)| {
            let cells = at.into_iter().map(|(c, (_, v))| (c, v)).collect();
            (tuple, cells)
        })
        .collect();
    tuples.sort_by(|a, b| a.0.cmp(&b.0));
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
    let mut pairs = std::collections::BTreeSet::new();
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

/// `Pi_F(I)` of `instance`, an instance on `S'`, with the tuple behind each
/// row; fresh row ids are allocated above every id in `floor`. See
/// [`functor_pi_along_with_provenance`].
pub(crate) fn pi_limit(
    instance: &FInstance,
    functor: &MigrationFunctor,
    max: usize,
    floor: impl IntoIterator<Item = RowId>,
) -> Result<PiLimit, RestrictError> {
    let mut limits: BTreeMap<&Name, TableLimit<'_>> = BTreeMap::new();
    for (table, fibre) in functor.table_fibres() {
        limits.insert(table, table_limit(instance, functor, table, fibre, max)?);
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
    let mut allocator = row_allocator(floor);
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

    fn fk(src: &str, tgt: &str, name: &str) -> Edge {
        Edge {
            src: src.into(),
            tgt: tgt.into(),
            kind: "fk".into(),
            name: Some(name.into()),
        }
    }

    fn column(table: &str, name: &str) -> Edge {
        Edge {
            src: table.into(),
            tgt: format!("{table}.{name}").into(),
            kind: "prop".into(),
            name: Some(name.into()),
        }
    }

    /// Tables of kind `record` with integer columns, and the given edges.
    fn tables(tables: &[(&str, &[&str])], edges: &[Edge]) -> Schema {
        let mut vertices: Vec<&str> = Vec::new();
        let mut all: Vec<Edge> = edges.to_vec();
        let names: Vec<String> = tables
            .iter()
            .flat_map(|(t, cs)| cs.iter().map(move |c| format!("{t}.{c}")))
            .collect();
        for (t, cs) in tables {
            vertices.push(t);
            for c in *cs {
                all.push(column(t, c));
            }
        }
        vertices.extend(names.iter().map(String::as_str));
        let mut schema = make_test_schema(&vertices, &all);
        for (id, vertex) in &mut schema.vertices {
            vertex.kind = if id.contains('.') {
                "integer"
            } else {
                "record"
            }
            .into();
        }
        schema
    }

    fn protocol() -> Protocol {
        Protocol {
            value_kinds: BTreeMap::from([(
                "integer".to_owned(),
                panproto_schema::ScalarKind::Int {
                    bits: 64,
                    signed: true,
                },
            )]),
            non_value_kinds: std::collections::BTreeSet::from(["record".to_owned()]),
            ..Protocol::default()
        }
    }

    /// Rows without cells, with the given ids.
    fn blank_rows(ids: &[u32]) -> Vec<(RowId, Row)> {
        ids.iter().map(|&i| (RowId(i), Row::new())).collect()
    }

    /// Rows holding `x = value`, with the given ids.
    fn x_rows(rows: &[(u32, i64)]) -> Vec<(RowId, Row)> {
        rows.iter()
            .map(|&(i, v)| (RowId(i), HashMap::from([("x".to_owned(), Value::Int(v))])))
            .collect()
    }

    /// A migration sending each `(source, target)` table, and each listed
    /// column of a source table to the same column of its image.
    fn migration_of(tables: &[(&str, &str)], columns: &[(&str, &str)]) -> CompiledMigration {
        let mut migration = CompiledMigration::default();
        for &(src, tgt) in tables {
            migration.vertex_remap.insert(src.into(), tgt.into());
            migration.surviving_verts.insert(tgt.into());
        }
        for &(table, name) in columns {
            let image = &migration.vertex_remap[table].clone();
            migration.vertex_remap.insert(
                format!("{table}.{name}").into(),
                format!("{image}.{name}").into(),
            );
            migration
                .edge_remap
                .insert(column(table, name), column(image, name));
        }
        migration
    }

    fn pi(
        inst: &FInstance,
        src: &Schema,
        tgt: &Schema,
        migration: &CompiledMigration,
        max: usize,
    ) -> Result<FInstance, AdjunctionError> {
        functor_pi(inst, src, &protocol(), tgt, &protocol(), migration, max)
    }

    fn pi_with_provenance(
        inst: &FInstance,
        src: &Schema,
        tgt: &Schema,
        migration: &CompiledMigration,
    ) -> (FInstance, PiProvenance) {
        let f = MigrationFunctor::new(src, &protocol(), tgt, &protocol(), migration).unwrap();
        functor_pi_along_with_provenance(inst, &f, 100).unwrap()
    }

    #[test]
    fn pi_at_a_target_table_outside_the_image_has_one_row() {
        let src = tables(&[("a", &["x"])], &[]);
        let tgt = tables(&[("a", &["x"]), ("z", &[])], &[]);
        let migration = migration_of(&[("a", "a")], &[("a", "x")]);
        let inst = FInstance::new()
            .with_rows("a", x_rows(&[(0, 5), (1, 6)]))
            .unwrap();
        let out = pi(&inst, &src, &tgt, &migration, 100).unwrap();
        assert_eq!(out.rows("a"), inst.rows("a"));
        assert_eq!(
            out.row_count("z"),
            1,
            "the limit of the empty diagram is one row"
        );
        let (id, row) = out.rows("z")[0];
        assert!(row.is_empty(), "the one row carries no cells");
        assert_eq!(id, RowId(2), "a fresh id above the input");
    }

    /// A target list column's element vertex is part of a cell, not a table
    /// outside the image, so the limit gives it no row.
    #[test]
    fn pi_gives_no_row_to_a_part_of_a_cell() {
        let items = Edge {
            src: "post.tags".into(),
            tgt: "post.tags:items".into(),
            kind: "items".into(),
            name: None,
        };
        let src = tables(&[("post", &["tags"])], &[]);
        let mut tgt = tables(&[("post", &["tags"])], std::slice::from_ref(&items));
        tgt.vertices.insert(
            "post.tags:items".into(),
            Vertex {
                id: "post.tags:items".into(),
                kind: "integer".into(),
                nsid: None,
            },
        );
        let migration = migration_of(&[("post", "post")], &[("post", "tags")]);
        let tags = Value::List(vec![Value::Int(1)]);
        let inst = FInstance::new()
            .with_rows(
                "post",
                [(RowId(0), HashMap::from([("tags".to_owned(), tags)]))],
            )
            .unwrap();
        assert_eq!(pi(&inst, &src, &tgt, &migration, 100).unwrap(), inst);
    }

    #[test]
    fn pi_at_a_target_only_relation_holds_every_pair() {
        let e = fk("a", "z", "e");
        let src = tables(&[("a", &[])], &[]);
        let tgt = tables(&[("a", &[]), ("z", &[])], std::slice::from_ref(&e));
        let migration = migration_of(&[("a", "a")], &[]);
        let inst = FInstance::new()
            .with_rows("a", blank_rows(&[0, 1]))
            .unwrap();
        let out = pi(&inst, &src, &tgt, &migration, 100).unwrap();
        let z = out.rows("z")[0].0;
        assert!(
            out.pairs(&e).iter().eq(&[(RowId(0), z), (RowId(1), z)]),
            "every pair of a row of a and the row of z: {:?}",
            out.pairs(&e)
        );
        assert!(matches!(
            pi(&inst, &src, &tgt, &migration, 1),
            Err(AdjunctionError::Pi(RestrictError::ProductSizeExceeded {
                actual: 2,
                limit: 1,
                ..
            }))
        ));
    }

    #[test]
    fn unmentioned_edges_are_outside_the_domain() {
        let e = fk("a", "b", "e");
        let src = tables(&[("a", &[]), ("b", &[])], std::slice::from_ref(&e));
        let tgt = tables(&[("m", &[])], &[]);
        let migration = migration_of(&[("a", "m"), ("b", "m")], &[]);
        let inst = FInstance::new()
            .with_rows("a", blank_rows(&[0, 1]))
            .and_then(|f| f.with_rows("b", blank_rows(&[2, 3])))
            .and_then(|f| f.with_foreign_key(e, [(RowId(0), RowId(3))]))
            .unwrap();
        let out = pi(&inst, &src, &tgt, &migration, 100).unwrap();
        assert_eq!(
            out.row_count("m"),
            4,
            "an unmentioned edge equalises nothing"
        );
        assert_eq!(out.pair_count(), 0);
    }

    /// Merging `a(x)` and `b(y)` into `m(x, y)` shares no column, so the
    /// limit is the product of the rows; adding a shared column `k` joins on
    /// it, and only on it.
    #[test]
    fn pi_joins_merged_tables_on_their_shared_columns_only() {
        let src = tables(&[("a", &["x"]), ("b", &["y"])], &[]);
        let tgt = tables(&[("m", &["x", "y"])], &[]);
        let migration = migration_of(&[("a", "m"), ("b", "m")], &[("a", "x"), ("b", "y")]);
        let y_rows = |rows: &[(u32, i64)]| -> Vec<(RowId, Row)> {
            rows.iter()
                .map(|&(i, v)| (RowId(i), HashMap::from([("y".to_owned(), Value::Int(v))])))
                .collect()
        };
        let inst = FInstance::new()
            .with_rows("a", x_rows(&[(0, 1), (1, 2)]))
            .and_then(|f| f.with_rows("b", y_rows(&[(2, 10), (3, 20), (4, 30)])))
            .unwrap();
        let (out, provenance) = pi_with_provenance(&inst, &src, &tgt, &migration);
        assert_eq!(out.row_count("m"), 6);
        let mut pairs = std::collections::BTreeSet::new();
        for (id, row) in out.rows("m") {
            assert_eq!(row.len(), 2);
            let (_, tuple) = &provenance.tuples[&id];
            assert_eq!(row["x"], inst.row(tuple[0]).unwrap()["x"]);
            assert_eq!(row["y"], inst.row(tuple[1]).unwrap()["y"]);
            pairs.insert((tuple[0], tuple[1]));
        }
        assert_eq!(pairs.len(), 6);

        // With a shared column `k`, only rows agreeing on `k` join.
        let src = tables(&[("a", &["x", "k"]), ("b", &["y", "k"])], &[]);
        let tgt = tables(&[("m", &["x", "y", "k"])], &[]);
        let migration = migration_of(
            &[("a", "m"), ("b", "m")],
            &[("a", "x"), ("b", "y"), ("a", "k"), ("b", "k")],
        );
        let row = |pairs: &[(&str, i64)]| -> Row {
            pairs
                .iter()
                .map(|(c, v)| ((*c).to_owned(), Value::Int(*v)))
                .collect()
        };
        let inst = FInstance::new()
            .with_rows(
                "a",
                [
                    (RowId(0), row(&[("x", 1), ("k", 0)])),
                    (RowId(1), row(&[("x", 2), ("k", 1)])),
                ],
            )
            .and_then(|f| {
                f.with_rows(
                    "b",
                    [
                        (RowId(2), row(&[("y", 10), ("k", 0)])),
                        (RowId(3), row(&[("y", 20), ("k", 0)])),
                        (RowId(4), row(&[("y", 30), ("k", 2)])),
                    ],
                )
            })
            .unwrap();
        let (out, provenance) = pi_with_provenance(&inst, &src, &tgt, &migration);
        let tuples: Vec<&Vec<RowId>> = provenance.tuples.values().map(|(_, t)| t).collect();
        assert_eq!(
            tuples,
            [&vec![RowId(0), RowId(2)], &vec![RowId(0), RowId(3)]]
        );
        assert!(
            out.rows("m")
                .iter()
                .all(|(_, r)| r["k"] == Value::Int(0) && r.len() == 3)
        );
    }

    /// A target column no source column is sent to has no value the limit
    /// could take, so Pi refuses before reading the instance.
    #[test]
    fn pi_refuses_a_column_outside_the_image() {
        let src = tables(&[("a", &[])], &[]);
        let tgt = tables(&[("a", &[]), ("z", &["v"])], &[]);
        let migration = migration_of(&[("a", "a")], &[]);
        // Not even a stray cell of the input is looked at.
        let inst = FInstance::new().with_rows("a", x_rows(&[(0, 1)])).unwrap();
        assert!(matches!(
            pi(&inst, &src, &tgt, &migration, 100),
            Err(AdjunctionError::Pi(RestrictError::AttributeOutsideImage { table, column }))
                if table == "z" && column == "v"
        ));
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
        let src = tables(&[("a", &[]), ("b", &[])], std::slice::from_ref(&l));
        let tgt = tables(&[("m", &[])], std::slice::from_ref(&loop_edge));
        let mut migration = migration_of(&[("a", "m"), ("b", "m")], &[]);
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
        let (out, provenance) = pi_with_provenance(&inst, &src, &tgt, &migration);
        assert_eq!(out.row_count("m"), 4);
        let rows: Vec<RowId> = out.rows("m").into_iter().map(|(id, _)| id).collect();
        let mut cones = std::collections::BTreeSet::new();
        for &x in &rows {
            for &y in &rows {
                let (a, b) = (provenance.tuples[&x].1[0], provenance.tuples[&y].1[1]);
                if inst.contains_pair(&fk("a", "b", "l"), a, b) {
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
            out.pairs(&loop_edge),
            &cones,
            "the loop holds every cone and nothing else"
        );
    }

    /// Left unmentioned, an internal edge of a fibre is outside the
    /// functor's domain and constrains nothing: the table is the bare
    /// product and the target loop nothing is carried to holds every pair.
    #[test]
    fn pi_over_a_fibre_with_an_internal_edge_is_its_limit() {
        let (inst, src, tgt, mut migration, loop_edge) = carried_loop();
        migration.edge_remap.clear();
        let out = pi(&inst, &src, &tgt, &migration, 100).unwrap();
        assert_eq!(out.row_count("m"), 4);
        assert_eq!(out.pairs(&loop_edge).len(), 16);
    }

    /// Along the identity migration as `panproto_mig::compile` states it
    /// (every table sent to itself, every foreign key and column listed as
    /// surviving), Pi returns its input: a post with no author and a post
    /// with two keep their rows, and an org chart whose root has no manager
    /// keeps every employee.
    #[test]
    fn pi_along_a_compiled_identity_keeps_partial_and_many_valued_keys() {
        let author = fk("post", "user", "author");
        let posts = FInstance::new()
            .with_rows("post", x_rows(&[(2, 0), (3, 1), (4, 0)]))
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
        for (inst, schema, key) in [
            (
                posts,
                tables(
                    &[("post", &["x"]), ("user", &[])],
                    std::slice::from_ref(&author),
                ),
                author.clone(),
            ),
            (
                chart,
                tables(&[("emp", &[])], std::slice::from_ref(&manager)),
                manager.clone(),
            ),
        ] {
            let mut migration = CompiledMigration::default();
            for table in ["post", "user", "emp"] {
                migration.surviving_verts.insert(table.into());
            }
            migration.surviving_edges.insert(key);
            migration.surviving_edges.insert(column("post", "x"));
            let (out, provenance) = pi_with_provenance(&inst, &schema, &schema, &migration);
            assert_eq!(out, inst);
            assert!(
                provenance
                    .tuples
                    .iter()
                    .all(|(row, (_, tuple))| tuple == &vec![*row])
            );
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
        let schema = tables(&[("a", &["n"]), ("b", &["n"])], std::slice::from_ref(&edge));
        let migration = CompiledMigration {
            surviving_verts: HashSet::from([Name::from("a"), Name::from("b")]),
            surviving_edges: HashSet::from([edge, column("a", "n"), column("b", "n")]),
            ..CompiledMigration::default()
        };
        let began = std::time::Instant::now();
        let out = pi(&inst, &schema, &schema, &migration, usize::MAX).unwrap();
        let took = began.elapsed();
        assert_eq!(out, inst, "Pi along the identity is the identity");
        assert!(
            took < std::time::Duration::from_secs(3),
            "Pi over {ROWS} pairs took {took:?}"
        );
    }

    /// Renaming the target tables in the image leaves every row id alone.
    #[test]
    fn results_do_not_depend_on_target_names() {
        let src = tables(&[("a", &[]), ("b", &[]), ("c", &[]), ("d", &[])], &[]);
        let run = |left: &str, right: &str| {
            let tgt = tables(&[(left, &[]), (right, &[])], &[]);
            let migration =
                migration_of(&[("a", left), ("b", left), ("c", right), ("d", right)], &[]);
            let inst = FInstance::new()
                .with_rows("a", blank_rows(&[0]))
                .and_then(|f| f.with_rows("b", blank_rows(&[1])))
                .and_then(|f| f.with_rows("c", blank_rows(&[2])))
                .and_then(|f| f.with_rows("d", blank_rows(&[3])))
                .unwrap();
            pi(&inst, &src, &tgt, &migration, 100).unwrap()
        };
        let (before, after) = (run("m", "n"), run("q", "p"));
        assert_eq!(
            before.rows("m").first().map(|r| r.0),
            after.rows("q").first().map(|r| r.0)
        );
        assert_eq!(
            before.rows("n").first().map(|r| r.0),
            after.rows("p").first().map(|r| r.0)
        );
    }
}
