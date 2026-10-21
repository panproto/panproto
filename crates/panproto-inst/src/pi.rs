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
/// The bound `max_product_size` is on products: at a target table whose
/// fibre has two or more tables, the product of their row counts in the
/// restricted instance, checked before any of the table's rows are built;
/// at a target relation to which no relation is carried, the product of
/// the row counts of its endpoint tables in the limit. A table whose fibre
/// is one table, or none, is never bounded, so along the identity functor
/// `Pi_F` returns its input whatever the bound.
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
/// [`RestrictError::ProductSizeExceeded`] when the product of the row
/// counts of a target table's fibre of two or more tables exceeds
/// `max_product_size`, or a relation to which no relation is carried would
/// hold more than `max_product_size` pairs (reported at its source table;
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

#[cfg(test)]
thread_local! {
    /// The join work done on this thread: the rows the joins have read and
    /// the partial tuples they have built, so a test can bound it.
    static JOIN_WORK: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Count `n` units of join work on this thread.
#[cfg(test)]
fn charge_join_work(n: usize) {
    JOIN_WORK.with(|work| work.set(work.get().saturating_add(n)));
}

/// Count `n` units of join work: nothing outside tests.
#[cfg(not(test))]
const fn charge_join_work(_n: usize) {}

/// One fibre table in the join at a target table: its position in the
/// fibre, the target columns its kept columns are sent to, and its rows
/// that can still be in a cone, read at those columns.
struct Component {
    /// Its position in the (ascending) fibre.
    position: usize,
    /// The target columns its kept columns are sent to.
    columns: Vec<String>,
    /// Its rows that can still be in a cone, ascending by id.
    rows: Vec<(RowId, AtTarget)>,
}

/// The canonical bytes of `at` at `columns`, or `None` when it lacks one.
fn key_at(at: &AtTarget, columns: &[&String]) -> Option<Vec<Vec<u8>>> {
    columns
        .iter()
        .map(|c| at.get(*c).map(|(bytes, _)| bytes.clone()))
        .collect()
}

/// The target columns two components share, ascending.
fn shared_columns<'c>(one: &'c Component, other: &Component) -> Vec<&'c String> {
    one.columns
        .iter()
        .filter(|c| other.columns.contains(c))
        .collect()
}

/// Drop, until nothing changes, every row of a component that agrees with
/// no row of some other component on the target columns the two share.
/// The join is unchanged (a dropped row is in no cone), and afterwards any
/// two components agree on what they share (pairwise consistency).
fn reduce(components: &mut [Component]) {
    let max_rounds = components
        .iter()
        .map(|component| component.rows.len())
        .sum::<usize>()
        .saturating_add(1);
    for _ in 0..max_rounds {
        let mut changed = false;
        for i in 0..components.len() {
            for j in 0..components.len() {
                if i == j {
                    continue;
                }
                let shared: Vec<String> = shared_columns(&components[i], &components[j])
                    .into_iter()
                    .cloned()
                    .collect();
                if shared.is_empty() {
                    continue;
                }
                let shared: Vec<&String> = shared.iter().collect();
                charge_join_work(components[i].rows.len() + components[j].rows.len());
                let keys: std::collections::HashSet<Vec<Vec<u8>>> = components[j]
                    .rows
                    .iter()
                    .filter_map(|(_, at)| key_at(at, &shared))
                    .collect();
                let before = components[i].rows.len();
                components[i]
                    .rows
                    .retain(|(_, at)| key_at(at, &shared).is_some_and(|k| keys.contains(&k)));
                if components[i].rows.len() != before {
                    changed = true;
                }
            }
        }
        if !changed {
            return;
        }
    }
    unreachable!("each changing reduction round removes at least one row");
}

/// The components that share target columns, grouped into connected
/// groups (as indices into `components`), each ascending, the groups
/// ordered by their least member.
fn connected_groups(components: &[Component]) -> Vec<Vec<usize>> {
    fn root(group_of: &mut [usize], mut i: usize) -> usize {
        for _ in 0..group_of.len() {
            if group_of[i] == i {
                return i;
            }
            group_of[i] = group_of[group_of[i]];
            i = group_of[i];
        }
        unreachable!("the parent forest is acyclic and points to smaller indices");
    }
    let mut group_of: Vec<usize> = (0..components.len()).collect();
    for i in 0..components.len() {
        for j in 0..i {
            if !shared_columns(&components[i], &components[j]).is_empty() {
                let (a, b) = (root(&mut group_of, i), root(&mut group_of, j));
                group_of[a.max(b)] = a.min(b);
            }
        }
    }
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..components.len() {
        let r = root(&mut group_of, i);
        groups.entry(r).or_default().push(i);
    }
    groups.into_values().collect()
}

/// A join tree of a connected group, by GYO reduction: repeatedly remove a
/// member (an ear) all of whose target columns shared with another
/// remaining member lie in one remaining member (its parent). Returns the
/// members in removal order, the last being the root, with each removed
/// member's parent; `None` when no ear is left before one member remains,
/// that is when the group's columns form a cyclic hypergraph.
fn join_tree(
    components: &[Component],
    group: &[usize],
) -> Option<(Vec<usize>, Vec<Option<usize>>)> {
    let mut parent: Vec<Option<usize>> = vec![None; components.len()];
    let mut remaining: Vec<usize> = group.to_vec();
    let mut order = Vec::with_capacity(group.len());
    while matches!(remaining.as_slice(), [_, _, ..]) {
        let ear = remaining.iter().enumerate().find_map(|(at, &e)| {
            let mut others = remaining.clone();
            others.remove(at);
            let linked: Vec<&String> = components[e]
                .columns
                .iter()
                .filter(|c| others.iter().any(|&f| components[f].columns.contains(c)))
                .collect();
            others
                .iter()
                .copied()
                .find(|&w| linked.iter().all(|c| components[w].columns.contains(c)))
                .map(|w| (at, e, w))
        });
        let (at, e, w) = ear?;
        parent[e] = Some(w);
        order.push(e);
        remaining.remove(at);
    }
    order.extend(remaining);
    Some((order, parent))
}

/// The number of cones of an acyclic group, from its join tree: each row's
/// weight is the product, over its children, of the summed weights of the
/// child rows that agree with it on the columns the two share; the count is
/// the summed weight of the root's rows. Saturates at `usize::MAX`.
fn tree_count(components: &[Component], removal: &[usize], parent: &[Option<usize>]) -> usize {
    let mut weights: Vec<Vec<u128>> = components
        .iter()
        .map(|c| vec![1_u128; c.rows.len()])
        .collect();
    for &child in removal {
        let Some(up) = parent[child] else {
            continue;
        };
        let shared = shared_columns(&components[child], &components[up]);
        charge_join_work(components[child].rows.len() + components[up].rows.len());
        let mut sums: HashMap<Vec<Vec<u8>>, u128> = HashMap::new();
        for ((_, at), weight) in components[child].rows.iter().zip(&weights[child]) {
            if let Some(key) = key_at(at, &shared) {
                let sum = sums.entry(key).or_insert(0);
                *sum = sum.saturating_add(*weight);
            }
        }
        for ((_, at), weight) in components[up].rows.iter().zip(&mut weights[up]) {
            let sum = key_at(at, &shared)
                .and_then(|key| sums.get(&key))
                .copied()
                .unwrap_or(0);
            *weight = weight.saturating_mul(sum);
        }
    }
    let total = removal.last().map_or(0, |&root| {
        weights[root]
            .iter()
            .fold(0_u128, |acc, w| acc.saturating_add(*w))
    });
    usize::try_from(total).unwrap_or(usize::MAX)
}

/// An order of a cyclic group's members in which each member after the
/// first shares as many target columns as possible with those before it
/// (ties: fewer rows, then the least member).
fn connected_order(components: &[Component], group: &[usize]) -> Vec<usize> {
    let mut seen: std::collections::BTreeSet<&String> = std::collections::BTreeSet::new();
    let mut remaining: Vec<usize> = group.to_vec();
    let mut order = Vec::with_capacity(group.len());
    while !remaining.is_empty() {
        let best = remaining
            .iter()
            .enumerate()
            .min_by_key(|(_, m)| {
                let linked = components[**m]
                    .columns
                    .iter()
                    .filter(|c| seen.contains(c))
                    .count();
                (std::cmp::Reverse(linked), components[**m].rows.len(), **m)
            })
            .map_or(0, |(at, _)| at);
        let member = remaining.remove(best);
        seen.extend(components[member].columns.iter());
        order.push(member);
    }
    order
}

/// One step of a join in a given order: the member it adds, the target
/// columns that member shares with the members before it, and its rows
/// indexed by their values there.
struct Step<'c> {
    /// The member added.
    member: usize,
    /// The target columns it shares with the members before it.
    shared: Vec<&'c String>,
    /// Its rows (indices) by their canonical bytes at `shared`.
    index: HashMap<Vec<Vec<u8>>, Vec<usize>>,
}

/// The steps of the join of members in `order`.
fn join_steps<'c>(components: &'c [Component], order: &[usize]) -> Vec<Step<'c>> {
    let mut seen: std::collections::BTreeSet<&String> = std::collections::BTreeSet::new();
    let mut steps = Vec::with_capacity(order.len());
    for &member in order {
        let shared: Vec<&String> = components[member]
            .columns
            .iter()
            .filter(|c| seen.contains(c))
            .collect();
        charge_join_work(components[member].rows.len());
        let mut index: HashMap<Vec<Vec<u8>>, Vec<usize>> = HashMap::new();
        for (n, (_, at)) in components[member].rows.iter().enumerate() {
            if let Some(key) = key_at(at, &shared) {
                index.entry(key).or_default().push(n);
            }
        }
        seen.extend(components[member].columns.iter());
        steps.push(Step {
            member,
            shared,
            index,
        });
    }
    steps
}

/// What a walk over the cones of a join gathers.
enum Gather {
    /// Only their number (saturating), read at the last step from its index.
    Count(usize),
    /// Each cone: the row (index) chosen at each step.
    Cones(Vec<Vec<usize>>),
}

/// The number of cones of the join given by `steps` (saturating).
fn count_cones(components: &[Component], steps: &[Step<'_>]) -> usize {
    let mut gather = Gather::Count(0);
    walk(components, steps, &mut gather);
    match gather {
        Gather::Count(count) => count,
        Gather::Cones(cones) => cones.len(),
    }
}

/// The cones of the join given by `steps`: the row (index) chosen at each
/// step.
fn list_cones(components: &[Component], steps: &[Step<'_>]) -> Vec<Vec<usize>> {
    let mut gather = Gather::Cones(Vec::new());
    walk(components, steps, &mut gather);
    match gather {
        Gather::Cones(cones) => cones,
        Gather::Count(_) => Vec::new(),
    }
}

/// Walk the cones of the join given by `steps`, depth first: at each step
/// the rows of its member that agree with the values assigned so far.
fn walk(components: &[Component], steps: &[Step<'_>], gather: &mut Gather) {
    fn go<'c>(
        level: usize,
        components: &'c [Component],
        steps: &[Step<'_>],
        assigned: &mut BTreeMap<&'c String, &'c Vec<u8>>,
        chosen: &mut Vec<usize>,
        gather: &mut Gather,
    ) {
        let step = &steps[level];
        let key: Option<Vec<Vec<u8>>> = step
            .shared
            .iter()
            .map(|c| assigned.get(*c).map(|bytes| (*bytes).clone()))
            .collect();
        let Some(matches) = key.and_then(|key| step.index.get(&key)) else {
            return;
        };
        charge_join_work(1);
        if level + 1 == steps.len() {
            match gather {
                Gather::Count(count) => *count = count.saturating_add(matches.len()),
                Gather::Cones(cones) => {
                    for &n in matches {
                        let mut cone = chosen.clone();
                        cone.push(n);
                        cones.push(cone);
                    }
                }
            }
            return;
        }
        for &n in matches {
            let (_, at) = &components[step.member].rows[n];
            let added: Vec<&String> = at
                .iter()
                .filter(|(c, _)| !assigned.contains_key(c))
                .map(|(c, _)| c)
                .collect();
            for column in &added {
                if let Some((bytes, _)) = at.get(*column) {
                    assigned.insert(column, bytes);
                }
            }
            chosen.push(n);
            go(level + 1, components, steps, assigned, chosen, gather);
            chosen.pop();
            for column in added {
                assigned.remove(column);
            }
        }
    }
    if steps.is_empty() {
        return;
    }
    go(
        0,
        components,
        steps,
        &mut BTreeMap::new(),
        &mut Vec::new(),
        gather,
    );
}

/// The cones at one target table: tuples with one row of each fibre table
/// whose values agree at every target column, ascending by tuple.
///
/// When the fibre has two or more tables, the product of their row counts
/// is checked against the bound before any tuple is built, so the join's
/// work is bounded by the input and the bound; a fibre of one table, or of
/// none, is not bounded. Then, a row of one fibre table that agrees with no
/// row of another on the target columns the two share is in no cone, so
/// such rows are dropped until none is left (an empty fibre table empties
/// the limit). The fibre tables fall into connected groups by shared target
/// columns, and the cones are the products of the groups' cones. A group
/// whose columns form an acyclic hypergraph (GYO reduction) is counted on
/// its join tree, and listing its cones in join-tree order extends every
/// partial tuple to a cone, because after the reduction its tables are
/// pairwise consistent, and a pairwise consistent acyclic join is globally
/// consistent. A cyclic group is counted and listed depth first, joining
/// each next table on the columns it shares with those before it. No tuple
/// is built when some group has no cone.
fn table_limit<'a>(
    instance: &FInstance,
    functor: &MigrationFunctor,
    table: &'a Name,
    fibre: Vec<&'a Name>,
    max: usize,
) -> Result<TableLimit<'a>, RestrictError> {
    if fibre.len() > 1 {
        let product = fibre
            .iter()
            .try_fold(1_usize, |acc, s| acc.checked_mul(instance.row_count(s)))
            .unwrap_or(usize::MAX);
        if product > max {
            return Err(RestrictError::ProductSizeExceeded {
                vertex: table.to_string(),
                actual: product,
                limit: max,
            });
        }
    }
    if fibre.is_empty() {
        // The limit of the empty diagram: one empty tuple.
        return Ok(TableLimit {
            table,
            fibre,
            tuples: vec![(Vec::new(), Row::new())],
        });
    }
    let mut components = fibre_components(instance, functor, &fibre);
    if let [only] = components.as_mut_slice() {
        // One fibre table: its rows are the cones.
        let rows = std::mem::take(&mut only.rows);
        charge_join_work(rows.len());
        let mut tuples: Vec<(Vec<RowId>, Row)> = rows
            .into_iter()
            .map(|(id, at)| (vec![id], at.into_iter().map(|(c, (_, v))| (c, v)).collect()))
            .collect();
        tuples.sort_by(|a, b| a.0.cmp(&b.0));
        return Ok(TableLimit {
            table,
            fibre,
            tuples,
        });
    }
    charge_join_work(components.iter().map(|c| c.rows.len()).sum());
    let empty = |fibre| TableLimit {
        table,
        fibre,
        tuples: Vec::new(),
    };
    if components.iter().any(|c| c.rows.is_empty()) {
        return Ok(empty(fibre));
    }
    reduce(&mut components);
    if components.iter().any(|c| c.rows.is_empty()) {
        return Ok(empty(fibre));
    }
    let plans = group_plans(&components);
    let total = plans
        .iter()
        .fold(1_usize, |acc, (_, count)| acc.saturating_mul(*count));
    if total == 0 {
        return Ok(empty(fibre));
    }
    let tuples = list_tuples(&components, &plans, fibre.len());
    Ok(TableLimit {
        table,
        fibre,
        tuples: tuple_cells(&components, tuples),
    })
}

/// The fibre tables of a target table, each with its rows read at the
/// target columns and the target columns its kept columns are sent to.
fn fibre_components(
    instance: &FInstance,
    functor: &MigrationFunctor,
    fibre: &[&Name],
) -> Vec<Component> {
    fibre
        .iter()
        .enumerate()
        .map(|(position, component)| {
            let rows: Vec<(RowId, AtTarget)> = instance
                .rows(component)
                .into_iter()
                .filter_map(|(id, cells)| Some((id, row_at_target(functor, component, cells)?)))
                .collect();
            let mut columns: Vec<String> = functor
                .kept_columns(component)
                .into_iter()
                .filter_map(|alpha| functor.column_image(component, alpha))
                .map(str::to_owned)
                .collect();
            columns.sort_unstable();
            columns.dedup();
            Component {
                position,
                columns,
                rows,
            }
        })
        .collect()
}

/// Each connected group of the (reduced) fibre tables in the order its
/// cones are listed, with its count of cones: the acyclic groups (cheap)
/// first, then the cyclic ones, stopping once a group has no cone.
fn group_plans(components: &[Component]) -> Vec<(Vec<usize>, usize)> {
    let mut plans: Vec<(Vec<usize>, usize)> = Vec::new();
    let mut cyclic: Vec<Vec<usize>> = Vec::new();
    for group in connected_groups(components) {
        match join_tree(components, &group) {
            Some((removal, parent)) => {
                let count = tree_count(components, &removal, &parent);
                let mut order = removal;
                order.reverse();
                plans.push((order, count));
            }
            None => cyclic.push(group),
        }
    }
    for group in cyclic {
        if plans.iter().any(|(_, count)| *count == 0) {
            break;
        }
        let order = connected_order(components, &group);
        let count = count_cones(components, &join_steps(components, &order));
        plans.push((order, count));
    }
    plans
}

/// The cones of the fibre, as tuples of `width` row ids: each group's
/// cones, then their product.
fn list_tuples(
    components: &[Component],
    plans: &[(Vec<usize>, usize)],
    width: usize,
) -> Vec<Vec<RowId>> {
    let mut tuples: Vec<Vec<RowId>> = vec![vec![RowId(0); width]];
    for (order, _) in plans {
        let steps = join_steps(components, order);
        let cones = list_cones(components, &steps);
        let mut next = Vec::with_capacity(tuples.len().saturating_mul(cones.len()));
        for tuple in &tuples {
            for cone in &cones {
                charge_join_work(1);
                let mut tuple = tuple.clone();
                for (step, &n) in steps.iter().zip(cone) {
                    let component = &components[step.member];
                    tuple[component.position] = component.rows[n].0;
                }
                next.push(tuple);
            }
        }
        tuples = next;
    }
    tuples
}

/// Each tuple with its cells, ascending by tuple: every fibre row read at
/// the target columns, a later fibre table's value standing where two
/// agree.
fn tuple_cells(components: &[Component], tuples: Vec<Vec<RowId>>) -> Vec<(Vec<RowId>, Row)> {
    let at_target: Vec<HashMap<RowId, &AtTarget>> = components
        .iter()
        .map(|c| c.rows.iter().map(|(id, at)| (*id, at)).collect())
        .collect();
    let mut tuples: Vec<(Vec<RowId>, Row)> = tuples
        .into_iter()
        .map(|tuple| {
            let mut cells = Row::new();
            for (position, id) in tuple.iter().enumerate() {
                if let Some(at) = at_target[position].get(id) {
                    cells.extend(at.iter().map(|(c, (_, v))| (c.clone(), v.clone())));
                }
            }
            (tuple, cells)
        })
        .collect();
    tuples.sort_by(|a, b| a.0.cmp(&b.0));
    tuples
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
        charge_join_work(1);
        let (Some(xs), Some(ys)) = (sources.get(&s), targets.get(&t)) else {
            continue;
        };
        for &x in xs {
            for &y in ys {
                charge_join_work(1);
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
    // Scanning source vertices in ascending order discovers a fibre's
    // collision at its second source. Thus the first collision is ordered
    // by `(second, first)`, while `target` remains the shared-image detail.
    if let Some((target, sources)) = fiber_map
        .iter()
        .filter(|(_, sources)| sources.len() > 1)
        .min_by(|(_, left), (_, right)| left[1].cmp(&right[1]).then_with(|| left[0].cmp(&right[0])))
    {
        return Err(RestrictError::NonInjectiveVertexMap {
            target: target.clone(),
            sources: sources.clone(),
        });
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

    // Remap nodes. The child index the transforms read through is built
    // once, so binding a node's children costs its own arcs, not a scan of
    // every arc.
    let children = crate::wtype::TreeChildren::new(instance);
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
            let ctx = crate::wtype::TransformContext::bound(
                None,
                &children,
                crate::wtype::InstanceRef::Tree(instance, id),
                &crate::wtype::transform_demand(&transforms),
            );
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
    use crate::wtype::FieldTransform;

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
    fn wtype_pi_applies_nonempty_field_transforms() {
        let schema = make_test_schema(&["root"], &[]);
        let mut root = Node::new(0, "root");
        root.extra_fields.insert("old".into(), Value::Int(7));
        let inst = WInstance::new(
            HashMap::from([(0, root)]),
            vec![],
            vec![],
            0,
            Name::from("root"),
        );
        let mut migration = CompiledMigration {
            surviving_verts: HashSet::from([Name::from("root")]),
            ..CompiledMigration::default()
        };
        migration.field_transforms.insert(
            Name::from("root"),
            vec![FieldTransform::RenameField {
                old_key: "old".into(),
                new_key: "new".into(),
            }],
        );

        let out = wtype_pi(&inst, &schema, &migration, usize::MAX).unwrap();
        assert_eq!(out.nodes[&0].extra_fields.get("new"), Some(&Value::Int(7)));
        assert!(!out.nodes[&0].extra_fields.contains_key("old"));
    }

    #[test]
    fn wtype_pi_ignores_an_arc_with_one_missing_endpoint() {
        let edge = Edge {
            src: "root".into(),
            tgt: "missing".into(),
            kind: "prop".into(),
            name: Some("child".into()),
        };
        let schema = make_test_schema(&["root"], &[]);
        let inst = WInstance::new(
            HashMap::from([(0, Node::new(0, "root"))]),
            vec![(0, 99, edge)],
            vec![],
            0,
            Name::from("root"),
        );
        let migration = CompiledMigration {
            surviving_verts: HashSet::from([Name::from("root")]),
            ..CompiledMigration::default()
        };

        let out = wtype_pi(&inst, &schema, &migration, usize::MAX).unwrap();
        assert!(out.arcs.is_empty());
    }

    #[test]
    fn wtype_pi_resolves_a_surviving_edge_when_exactly_one_endpoint_changes() {
        for remap_parent in [true, false] {
            let source_edge = Edge {
                src: "parent".into(),
                tgt: "child".into(),
                kind: "prop".into(),
                name: Some("link".into()),
            };
            let (parent_image, child_image) = if remap_parent {
                (Name::from("parent-image"), Name::from("child"))
            } else {
                (Name::from("parent"), Name::from("child-image"))
            };
            let target_edge = Edge {
                src: parent_image.clone(),
                tgt: child_image.clone(),
                kind: "prop".into(),
                name: Some("resolved".into()),
            };
            let schema = make_test_schema(
                &[parent_image.as_str(), child_image.as_str()],
                std::slice::from_ref(&target_edge),
            );
            let inst = WInstance::new(
                HashMap::from([(0, Node::new(0, "parent")), (1, Node::new(1, "child"))]),
                vec![(0, 1, source_edge.clone())],
                vec![],
                0,
                Name::from("parent"),
            );
            let mut migration = CompiledMigration {
                surviving_edges: HashSet::from([source_edge]),
                ..CompiledMigration::default()
            };
            if remap_parent {
                migration
                    .vertex_remap
                    .insert(Name::from("parent"), parent_image);
                migration.surviving_verts.insert(Name::from("child"));
            } else {
                migration
                    .vertex_remap
                    .insert(Name::from("child"), child_image);
                migration.surviving_verts.insert(Name::from("parent"));
            }

            let out = wtype_pi(&inst, &schema, &migration, usize::MAX).unwrap();
            assert_eq!(
                out.arcs,
                vec![(0, 1, target_edge)],
                "remap_parent={remap_parent}"
            );
        }
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

    /// The fresh row of a table outside the image comes from the row floor
    /// and never takes `u32::MAX`: at every floor in the last four ids, the
    /// row is the floor itself below `u32::MAX` and the typed exhaustion
    /// error at it.
    #[test]
    fn pi_withholds_the_top_row_id() {
        let src = tables(&[("a", &["x"])], &[]);
        let tgt = tables(&[("a", &["x"]), ("z", &[])], &[]);
        let migration = migration_of(&[("a", "a")], &[("a", "x")]);
        for floor in (u32::MAX - 3)..=u32::MAX {
            let inst = FInstance::new()
                .with_rows("a", x_rows(&[(0, 5), (floor - 1, 6)]))
                .unwrap();
            let out = pi(&inst, &src, &tgt, &migration, 100);
            if floor < u32::MAX {
                let out = out.unwrap();
                assert_eq!(out.rows("z")[0].0, RowId(floor), "the floor itself");
            } else {
                assert!(
                    matches!(
                        out,
                        Err(AdjunctionError::Pi(RestrictError::RowIdsExhausted))
                    ),
                    "floor {floor}: {out:?}"
                );
            }
        }
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

    /// The bound is on the product of the fibre's row counts, not on the
    /// join: `a(k)` and `b(k)` merged into `m(k)` with `a.k = {0, 1, 2}` and
    /// `b.k = {0, 5, 6}` join to one row, but their product of 9 exceeds 4.
    #[test]
    fn pi_refuses_a_fibre_whose_product_exceeds_the_bound_though_its_join_is_small() {
        let src = tables(&[("a", &["k"]), ("b", &["k"])], &[]);
        let tgt = tables(&[("m", &["k"])], &[]);
        let migration = migration_of(&[("a", "m"), ("b", "m")], &[("a", "k"), ("b", "k")]);
        let inst = FInstance::new()
            .with_rows("a", int_rows("k", [(0, 0), (1, 1), (2, 2)]))
            .and_then(|f| f.with_rows("b", int_rows("k", [(3, 0), (4, 5), (5, 6)])))
            .unwrap();
        let out = pi(&inst, &src, &tgt, &migration, 4);
        assert!(
            matches!(
                &out,
                Err(AdjunctionError::Pi(RestrictError::ProductSizeExceeded {
                    vertex,
                    actual: 9,
                    limit: 4,
                })) if vertex == "m"
            ),
            "{out:?}"
        );
        assert_eq!(
            pi(&inst, &src, &tgt, &migration, 9).unwrap().row_count("m"),
            1
        );
    }

    /// A table whose fibre is one table is never bounded: Pi along the
    /// identity returns a five-row table under the bound 4 unchanged, and a
    /// table outside the image gets its one row under the bound 0.
    #[test]
    fn pi_bounds_no_table_whose_fibre_has_fewer_than_two_tables() {
        let schema = tables(&[("a", &["k"])], &[]);
        let inst = FInstance::new()
            .with_rows("a", int_rows("k", (0..5).map(|i| (i, i64::from(i)))))
            .unwrap();
        let identity = MigrationFunctor::identity(&schema, &protocol()).unwrap();
        assert_eq!(functor_pi_along(&inst, &identity, 4).unwrap(), inst);
        assert_eq!(functor_pi_along(&inst, &identity, 0).unwrap(), inst);
        let tgt = tables(&[("a", &["k"]), ("z", &[])], &[]);
        let migration = migration_of(&[("a", "a")], &[("a", "k")]);
        let out = pi(&inst, &schema, &tgt, &migration, 0).unwrap();
        assert_eq!((out.row_count("a"), out.row_count("z")), (5, 1));
    }

    /// The join work done on this thread while `run` runs.
    fn join_work<T>(run: impl FnOnce() -> T) -> (T, usize) {
        let before = JOIN_WORK.with(std::cell::Cell::get);
        let out = run();
        (out, JOIN_WORK.with(std::cell::Cell::get) - before)
    }

    /// A row already expressed at target columns.
    fn at_target(values: impl IntoIterator<Item = (&'static str, i64)>) -> AtTarget {
        values
            .into_iter()
            .map(|(column, value)| {
                let value = Value::Int(value);
                (column.to_owned(), (value.canonical_bytes(), value))
            })
            .collect()
    }

    /// A join component with target columns and rows supplied directly.
    fn component(
        position: usize,
        columns: &[&str],
        rows: impl IntoIterator<Item = (u32, AtTarget)>,
    ) -> Component {
        Component {
            position,
            columns: columns.iter().map(|column| (*column).to_owned()).collect(),
            rows: rows.into_iter().map(|(id, row)| (RowId(id), row)).collect(),
        }
    }

    #[test]
    fn join_reduction_reaches_the_fixed_point_and_charges_each_scan() {
        let mut components = vec![
            component(
                0,
                &["x"],
                [(0, at_target([("x", 1)])), (1, at_target([("x", 2)]))],
            ),
            component(
                1,
                &["x", "y"],
                [
                    (2, at_target([("x", 1), ("y", 1)])),
                    (3, at_target([("x", 2), ("y", 2)])),
                ],
            ),
            component(2, &["y"], [(4, at_target([("y", 1)]))]),
        ];

        let ((), work) = join_work(|| reduce(&mut components));
        assert_eq!(
            components
                .iter()
                .map(|part| part.rows.iter().map(|(id, _)| *id).collect::<Vec<_>>())
                .collect::<Vec<_>>(),
            vec![vec![RowId(0)], vec![RowId(2)], vec![RowId(4)]]
        );
        assert_eq!(work, 30);
    }

    #[test]
    fn connected_groups_follow_shared_column_chains() {
        let components = vec![
            component(0, &["x"], []),
            component(1, &["z"], []),
            component(2, &["x", "y"], []),
            component(3, &["y"], []),
        ];

        assert_eq!(connected_groups(&components), vec![vec![0, 2, 3], vec![1]]);
    }

    #[test]
    fn join_tree_accepts_a_chain_and_rejects_a_cycle() {
        let chain = vec![
            component(0, &["x"], []),
            component(1, &["x", "y"], []),
            component(2, &["y"], []),
        ];
        let (order, parent) = join_tree(&chain, &[0, 1, 2]).unwrap();
        assert_eq!(order, vec![0, 1, 2]);
        assert_eq!(parent, vec![Some(1), Some(2), None]);

        let cycle = vec![
            component(0, &["a", "b"], []),
            component(1, &["b", "c"], []),
            component(2, &["a", "c"], []),
        ];
        assert!(join_tree(&cycle, &[0, 1, 2]).is_none());
    }

    #[test]
    fn tree_and_walk_counts_agree_on_a_join_with_multiplicity() {
        let components = vec![
            component(
                0,
                &["x"],
                [
                    (0, at_target([("x", 1)])),
                    (1, at_target([("x", 1)])),
                    (2, at_target([("x", 2)])),
                ],
            ),
            component(
                1,
                &["x", "y"],
                [
                    (3, at_target([("x", 1), ("y", 10)])),
                    (4, at_target([("x", 2), ("y", 20)])),
                ],
            ),
            component(
                2,
                &["y"],
                [(5, at_target([("y", 10)])), (6, at_target([("y", 20)]))],
            ),
        ];
        let (removal, parent) = join_tree(&components, &[0, 1, 2]).unwrap();
        let (tree_total, work) = join_work(|| tree_count(&components, &removal, &parent));
        assert_eq!(tree_total, 3);
        assert_eq!(work, 9);

        let order = connected_order(&components, &[0, 1, 2]);
        let steps = join_steps(&components, &order);
        assert_eq!(count_cones(&components, &steps), 3);
    }

    /// Rows holding `column = value` for each `(id, value)`.
    fn int_rows(column: &str, rows: impl IntoIterator<Item = (u32, i64)>) -> Vec<(RowId, Row)> {
        rows.into_iter()
            .map(|(i, v)| {
                (
                    RowId(i),
                    HashMap::from([(column.to_owned(), Value::Int(v))]),
                )
            })
            .collect()
    }

    /// `a(x)` and `b(y)` share no target column and `c(x)` is empty, so the
    /// limit at `d(x, y)` is empty: the join must not first build the
    /// `700 * 700` partial tuples of `a` and `b`.
    #[test]
    fn pi_does_not_build_a_product_that_a_later_component_empties() {
        let src = tables(&[("a", &["x"]), ("b", &["y"]), ("c", &["x"])], &[]);
        let tgt = tables(&[("d", &["x", "y"])], &[]);
        let migration = migration_of(
            &[("a", "d"), ("b", "d"), ("c", "d")],
            &[("a", "x"), ("b", "y"), ("c", "x")],
        );
        let inst = FInstance::new()
            .with_rows("a", int_rows("x", (0..700).map(|i| (i, i64::from(i)))))
            .and_then(|f| f.with_rows("b", int_rows("y", (700..1400).map(|i| (i, 0)))))
            .and_then(|f| f.with_rows("c", Vec::new()))
            .unwrap();
        let (out, work) = join_work(|| pi(&inst, &src, &tgt, &migration, 4));
        assert_eq!(out.unwrap().row_count("d"), 0);
        assert!(
            work <= 10_000,
            "{work} units of join work for an empty limit"
        );
    }

    /// `a(x, k)`, `b(y, k)` and `c(x)` hold 700 rows each, so the product
    /// of the fibre of `d(x, y, k)` is `700^3`: refused under the bound 4
    /// with that product, before any join work.
    #[test]
    fn pi_refuses_an_over_bound_fibre_before_joining_it() {
        let src = tables(
            &[("a", &["x", "k"]), ("b", &["y", "k"]), ("c", &["x"])],
            &[],
        );
        let tgt = tables(&[("d", &["x", "y", "k"])], &[]);
        let migration = migration_of(
            &[("a", "d"), ("b", "d"), ("c", "d")],
            &[("a", "x"), ("a", "k"), ("b", "y"), ("b", "k"), ("c", "x")],
        );
        let k_rows = |column: &str, ids: std::ops::Range<u32>| -> Vec<(RowId, Row)> {
            ids.map(|i| {
                let row: Row = HashMap::from([
                    (column.to_owned(), Value::Int(i64::from(i))),
                    ("k".to_owned(), Value::Int(0)),
                ]);
                (RowId(i), row)
            })
            .collect()
        };
        let inst = FInstance::new()
            .with_rows("a", k_rows("x", 0..700))
            .and_then(|f| f.with_rows("b", k_rows("y", 700..1400)))
            .and_then(|f| {
                f.with_rows(
                    "c",
                    int_rows("x", (1400..2100).map(|i| (i, i64::from(i) - 1400))),
                )
            })
            .unwrap();
        let (out, work) = join_work(|| pi(&inst, &src, &tgt, &migration, 4));
        assert!(
            matches!(
                out,
                Err(AdjunctionError::Pi(RestrictError::ProductSizeExceeded {
                    actual: 343_000_000,
                    limit: 4,
                    ..
                }))
            ),
            "{out:?}"
        );
        assert_eq!(work, 0, "{work} units of join work before the refusal");
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
        let (out, work) = join_work(|| pi(&inst, &schema, &schema, &migration, usize::MAX));
        assert_eq!(out.unwrap(), inst, "Pi along the identity is the identity");
        // Counted, not timed: the rows of both tables are read once and each
        // pair is looked up once and matched once, so the work is linear in
        // the input (a quadratic join would be about `ROWS * ROWS`).
        let linear = 5 * usize::try_from(ROWS).unwrap();
        assert!(
            work <= linear,
            "Pi over {ROWS} pairs did {work} units of join work"
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
