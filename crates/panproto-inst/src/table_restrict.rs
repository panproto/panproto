//! The table restrict and extend engines.
//!
//! [`functor_restrict_with_plan`] is the one table restrict engine: it reads
//! a [`RestrictPlan`] compiled for tables
//! ([`RestrictPlan::compile_for_tables`]) and returns the view with its
//! [`TableComplement`]. It runs the graph engine's stages over rows and
//! foreign-key pairs, reading each [`RowId`] as a node id and each pair as
//! an edge, on the migrations where the restrict's survival rule and the
//! table functor of the migration agree, so its result is also the entity
//! part of `Sigma` along that functor. The schema-taking
//! [`functor_restrict_with_complement`] and [`functor_restrict`] validate
//! both boundaries around it, and [`functor_restrict_validated`] does the
//! same over plans compiled already. [`functor_extend`] and
//! [`functor_extend_validated`] push a table instance forward and validate
//! the output.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

use panproto_gat::Name;
use panproto_schema::{Edge, Protocol, Schema};

use crate::contraction_plan::PairResolution;
use crate::error::RestrictError;
use crate::functor::{FInstance, PairSet, Row, same_value};
use crate::metadata::Node;
use crate::rel_schema::RelSchema;
use crate::restrict_boundary::{
    BoundaryProtocols, BoundarySide, boundary_plan, require_passed, require_schema, unlimited,
};
use crate::restrict_plan::{EdgeFate, RestrictPlan, ReverseAnchors, floor_above};
use crate::table_complement::{DroppedRow, TableComplement};
use crate::validation::ValidationPlan;
use crate::value::Value;
use crate::view_ref::RowId;
use crate::wtype::{CompiledMigration, InstanceRef, TransformContext};

/// The restrict of a table instance along a plan compiled for tables: the
/// single table engine.
///
/// The precondition is the caller's: this function validates nothing, so
/// the instance should be valid against the plan's source schema
/// ([`functor_restrict_with_complement`] validates both boundaries). The
/// complement is unstamped; the lens stamps it.
///
/// Rows are read in ascending id. A row survives when its table survives
/// the vertex map and its table's survival predicate, evaluated over its
/// original cells and the rows its pairs reach, is `Bool(true)`; it keeps
/// its id, moves to the image of its table, and is transformed: value
/// transforms, then the plan's cell fates (columns the migration drops or
/// renames), then the coercion of the columns its transforms wrote. A pair
/// between surviving rows travels along its foreign key's fate: kept,
/// relabelled, or dropped; a pair with a dropped row is dropped. Then
/// contraction: each surviving row reaches, through dropped rows (pairs in
/// ascending foreign key and target), surviving rows, and each pair so
/// formed is joined by one synthesized pair of its resolution. A foreign
/// key is a set of pairs, so pairs that two source keys carry onto one view
/// key, or that a direct pair and a contraction both produce, are one.
///
/// # Errors
///
/// [`RestrictError::TablePlanRequired`] for a plan compiled without the
/// table functor. Then, in ascending row id, the refusals of survival
/// predicates and transforms; in ascending foreign key and pair,
/// [`RestrictError::IncoherentEdgeImage`] for a pair whose key is no source
/// schema edge, and [`RestrictError::NotTableRepresentable`],
/// [`RestrictError::NoEdgeFound`] or [`RestrictError::AmbiguousEdge`] for a
/// key whose fate cannot be a pair; and, in ascending pair of rows, the same
/// for a contracted pair. The table engine locates no error.
pub fn functor_restrict_with_plan(
    instance: &FInstance,
    plan: &RestrictPlan,
) -> Result<(FInstance, TableComplement), RestrictError> {
    if plan.table_functor().is_none() {
        return Err(RestrictError::TablePlanRequired);
    }
    let mut run = TableRun::new(instance, plan);
    run.rows()?;
    run.pairs()?;
    run.contraction()?;
    Ok((run.view, run.complement))
}

/// The state of one table restrict.
struct TableRun<'a> {
    instance: &'a FInstance,
    plan: &'a RestrictPlan,
    migration: &'a CompiledMigration,
    reverse: ReverseAnchors<'a>,
    complement: TableComplement,
    view: FInstance,
    /// For each view pair, the source keys that carried it.
    carriers: BTreeMap<(Edge, RowId, RowId), BTreeSet<Edge>>,
}

impl<'a> TableRun<'a> {
    fn new(instance: &'a FInstance, plan: &'a RestrictPlan) -> Self {
        let mut complement = TableComplement::empty();
        complement.id_floor = floor_above(instance.max_row_id().map(|r| r.0));
        Self {
            instance,
            plan,
            migration: plan.migration(),
            reverse: ReverseAnchors::new(plan.migration()),
            complement,
            view: FInstance::new(),
            carriers: BTreeMap::new(),
        }
    }

    /// Survival and transforms, row by row.
    fn rows(&mut self) -> Result<(), RestrictError> {
        for id in self.instance.row_ids() {
            let (Some(table), Some(row)) = (self.instance.table_of(id), self.instance.row(id))
            else {
                continue;
            };
            match self.row(id, table, row)? {
                Some((image, cells)) => self.view.place_row(image, id, cells),
                None => {
                    self.complement.dropped_rows.insert(
                        id,
                        DroppedRow {
                            table: table.clone(),
                            row: row.clone(),
                        },
                    );
                }
            }
        }
        Ok(())
    }

    /// The view of one row, its table and cells, or `None` when it does not
    /// survive.
    fn row(
        &mut self,
        id: RowId,
        table: &Name,
        row: &Row,
    ) -> Result<Option<(Name, Row)>, RestrictError> {
        if !self.plan.anchor_survives(table) {
            return Ok(None);
        }
        let has_expressions = self.migration.conditional_survival.contains_key(table)
            || !self.migration.value_transforms(table).is_empty();
        let ctx = if has_expressions {
            TransformContext::bound(
                Some(self.plan.source()),
                self.instance,
                InstanceRef::Table(self.instance, id),
                &self.plan.demand(table),
            )
        } else {
            TransformContext::detached()
        };
        if !self.plan.passes_predicate(table, &ctx, row)? {
            return Ok(None);
        }
        let image = self.plan.target_anchor(table).clone();
        let mut node = Node::new(id.0, image.clone());
        node.extra_fields.clone_from(row);
        self.plan.transform(&mut node, table, &ctx, true)?;
        if !self.reverse.recovers(&image, table) {
            self.complement.original_tables.insert(id, table.clone());
        }
        // Each source cell is compared with the view's cell of the same
        // name: a column the view holds under no cell of its name,
        // renamed or dropped, is a dropped cell, and one it holds with
        // another value is an original cell. Put rebuilds the row from
        // these and the view's cells of the source table's columns, so get
        // paired with the complement is injective on sources whatever the
        // transforms did, value-dependent renames included.
        for (column, value) in row {
            match node.extra_fields.get(column) {
                None => {
                    self.complement
                        .dropped_cells
                        .insert((id, column.clone()), value.clone());
                }
                Some(now) if !same_value(now, value) => {
                    self.complement
                        .original_cells
                        .insert((id, column.clone()), value.clone());
                }
                Some(_) => {}
            }
        }
        Ok(Some((image, node.extra_fields)))
    }

    /// Pairs between surviving rows, by their foreign key's fate.
    fn pairs(&mut self) -> Result<(), RestrictError> {
        let instance = self.instance;
        for (edge, pairs) in instance.foreign_keys() {
            for &(src, tgt) in pairs {
                let kept = self.view.table_of(src).is_some() && self.view.table_of(tgt).is_some();
                if !kept {
                    self.drop_pair(edge, src, tgt);
                    continue;
                }
                match self.plan.edge_image.fate(edge) {
                    None => {
                        return Err(RestrictError::IncoherentEdgeImage {
                            source_edge: Box::new(edge.clone()),
                            image: Box::new(edge.clone()),
                        });
                    }
                    Some(EdgeFate::Keep(image)) => {
                        self.view.place_pair(image.clone(), src, tgt);
                        self.carriers
                            .entry((image.clone(), src, tgt))
                            .or_default()
                            .insert(edge.clone());
                    }
                    Some(EdgeFate::Drop) => self.drop_pair(edge, src, tgt),
                    Some(EdgeFate::Expand { .. }) => {
                        return Err(RestrictError::NotTableRepresentable {
                            reason: format!("edge {edge:?} expands into a path"),
                        });
                    }
                    Some(EdgeFate::Unresolved(reason)) => {
                        let (Some(a), Some(b)) = (self.view.table_of(src), self.view.table_of(tgt))
                        else {
                            continue;
                        };
                        return Err(self.plan.unresolved(a, b, *reason));
                    }
                }
            }
        }
        for ((image, src, tgt), carriers) in std::mem::take(&mut self.carriers) {
            let maps_back =
                self.plan.edge_image.preimages(&image).len() == 1 && carriers.len() == 1;
            if !maps_back {
                self.complement
                    .original_fk_edges
                    .insert((image, src, tgt), carriers);
            }
        }
        Ok(())
    }

    fn drop_pair(&mut self, edge: &Edge, src: RowId, tgt: RowId) {
        self.complement
            .dropped_fk_pairs
            .entry(edge.clone())
            .or_default()
            .insert((src, tgt));
    }

    /// The surviving rows each surviving row reaches through dropped rows.
    fn realized_pairs(&self) -> BTreeSet<(RowId, RowId)> {
        let instance = self.instance;
        let survives = |row: RowId| self.view.table_of(row).is_some();
        let mut out_pairs: HashMap<RowId, Vec<RowId>> = HashMap::new();
        for (_, pairs) in instance.foreign_keys() {
            for &(src, tgt) in pairs {
                out_pairs.entry(src).or_default().push(tgt);
            }
        }
        let mut realized = BTreeSet::new();
        for source in self.view.row_ids() {
            let mut visited: HashSet<RowId> = HashSet::new();
            let mut queue: VecDeque<RowId> = VecDeque::new();
            let mut step = |from: RowId, queue: &mut VecDeque<RowId>, first: bool| {
                for &next in out_pairs.get(&from).map_or(&[][..], Vec::as_slice) {
                    if survives(next) {
                        if !first {
                            realized.insert((source, next));
                        }
                    } else if visited.insert(next) {
                        queue.push_back(next);
                    }
                }
            };
            step(source, &mut queue, true);
            while let Some(at) = queue.pop_front() {
                step(at, &mut queue, false);
            }
        }
        realized
    }

    /// Contraction through dropped rows.
    fn contraction(&mut self) -> Result<(), RestrictError> {
        for (src, tgt) in self.realized_pairs() {
            let (Some(src_table), Some(tgt_table)) =
                (self.instance.table_of(src), self.instance.table_of(tgt))
            else {
                continue;
            };
            let Some(pair) = self.plan.contraction.pair(src_table, tgt_table) else {
                return Err(RestrictError::NoEdgeFound {
                    src: self.plan.target_anchor(src_table).to_string(),
                    tgt: self.plan.target_anchor(tgt_table).to_string(),
                });
            };
            match &pair.resolution {
                PairResolution::Label(edge) => {
                    self.view.place_pair(edge.clone(), src, tgt);
                    self.complement
                        .synthesized_fk_pairs
                        .insert((edge.clone(), src, tgt));
                }
                PairResolution::Expand { .. } => {
                    return Err(RestrictError::NotTableRepresentable {
                        reason: format!(
                            "the contracted pair ({src_table}, {tgt_table}) expands into a path"
                        ),
                    });
                }
                PairResolution::Unresolved(reason) => {
                    return Err(self
                        .plan
                        .unresolved(&pair.target.0, &pair.target.1, *reason));
                }
            }
        }
        Ok(())
    }
}

/// The restrict of a table instance from `src_schema` to `tgt_schema` along
/// `migration`, validating both boundaries, with its complement.
///
/// The input is validated against the source schema under
/// `protocols.src`; a report that does not pass is
/// [`RestrictError::Validation`] on the input side, and nothing is compiled
/// or run. Otherwise the plan is compiled for tables
/// ([`RestrictPlan::compile_for_tables`], which builds the table functor of
/// the migration from both schemas and both protocols), the engine
/// ([`functor_restrict_with_plan`]) runs, and the output is validated
/// against the target schema under `protocols.tgt`; a report that does not
/// pass is `Validation` on the output side.
///
/// # Errors
///
/// As stated above.
pub fn functor_restrict_with_complement(
    instance: &FInstance,
    src_schema: &Schema,
    tgt_schema: &Schema,
    migration: &CompiledMigration,
    protocols: BoundaryProtocols<'_>,
) -> Result<(FInstance, TableComplement), RestrictError> {
    let (budget, external) = unlimited();
    let src_plan = boundary_plan(src_schema, protocols.src);
    require_passed(
        src_plan.validate_table(instance, &budget, &external),
        BoundarySide::Input,
    )?;
    let plan = RestrictPlan::compile_for_tables(src_schema, tgt_schema, migration, protocols)?;
    let restricted = functor_restrict_with_plan(instance, &plan)?;
    let tgt_plan = boundary_plan(tgt_schema, protocols.tgt);
    require_passed(
        tgt_plan.validate_table(&restricted.0, &budget, &external),
        BoundarySide::Output,
    )?;
    Ok(restricted)
}

/// The restrict of a table instance, validating both boundaries: the first
/// projection of [`functor_restrict_with_complement`].
///
/// # Errors
///
/// As [`functor_restrict_with_complement`].
pub fn functor_restrict(
    instance: &FInstance,
    src_schema: &Schema,
    tgt_schema: &Schema,
    migration: &CompiledMigration,
    protocols: BoundaryProtocols<'_>,
) -> Result<FInstance, RestrictError> {
    functor_restrict_with_complement(instance, src_schema, tgt_schema, migration, protocols)
        .map(|(view, _)| view)
}

/// [`functor_restrict_with_complement`] over plans compiled already: the
/// input validated by `src_plan`, the engine run on `plan` (which must be
/// compiled for tables), the output validated by `tgt_plan`.
///
/// # Errors
///
/// [`RestrictError::PlanSchemaMismatch`] before anything runs, on the input
/// side when `src_plan` was compiled for another schema than `plan`'s
/// source and then on the output side for `tgt_plan` and `plan`'s target;
/// otherwise as [`functor_restrict_with_complement`] after compilation.
pub fn functor_restrict_validated(
    instance: &FInstance,
    src_plan: &ValidationPlan,
    tgt_plan: &ValidationPlan,
    plan: &RestrictPlan,
) -> Result<(FInstance, TableComplement), RestrictError> {
    require_schema(src_plan, &plan.src_schema, BoundarySide::Input)?;
    require_schema(tgt_plan, &plan.tgt_schema, BoundarySide::Output)?;
    let (budget, external) = unlimited();
    require_passed(
        src_plan.validate_table(instance, &budget, &external),
        BoundarySide::Input,
    )?;
    let restricted = functor_restrict_with_plan(instance, plan)?;
    require_passed(
        tgt_plan.validate_table(&restricted.0, &budget, &external),
        BoundarySide::Output,
    )?;
    Ok(restricted)
}

/// The extend operation for table instances (`Sigma_F`), classifying the
/// target schema under `tgt_protocol` and validating the output against it.
///
/// This is the left Kan extension: given an instance of the source schema
/// and a migration mapping (source -> target), produce an instance of the
/// target schema by moving every table to its image. A vertex in the
/// migration's domain (the vertices `vertex_remap` renames, together with
/// those `surviving_verts` lists that it neither renames nor names as some
/// other vertex's image) goes to the image `vertex_remap` names, or keeps
/// its name when it survives unrenamed; a vertex the migration does not
/// mention at all keeps its name. A vertex the migration names only as a
/// target, the image of some other vertex that it does not rename itself,
/// has no image of its own, and extend refuses rows held there rather than
/// merge them into that fibre unasked; a surviving target vertex no row
/// reaches is sent to the empty set, so it has no table. Rows keep their
/// [`RowId`]s, so the coproduct over a merging fibre is a disjoint union
/// with no renumbering, and the operation creates no rows. A row takes its
/// source table's value transforms as the tree extend applies them. Within
/// each target table the rows are padded to the union of their columns with
/// [`Value::Null`]. Edges are read the same way: a source edge (one
/// `edge_remap` remaps, or a surviving edge it neither remaps nor names as
/// another edge's image) travels to its image, an edge the migration does
/// not mention keeps its kind and name between the images of its
/// endpoints, and an edge the migration names only as a target, the image
/// of some other edge that it does not remap itself, has no image of its
/// own, so extend refuses pairs held there. Relabelling an unmentioned edge
/// can land it on an edge that something else already reaches: the image of
/// a source edge (whether or not that source edge holds pairs), or the
/// relabelling of another unmentioned edge the instance holds pairs on.
/// Extend refuses such an edge too rather than merge its pairs there
/// unasked; the only merges it makes are those of source edges that
/// `edge_remap` sends to one image. An unmentioned edge whose relabelling is
/// a source edge the migration remaps away is no merge, so it travels. A
/// pair is kept when its rows lie in the carried edge's endpoint tables.
///
/// The target must have a relation-ified form ([`RelSchema::new`]). The
/// input's schema is not a parameter, so its callers validate the input at
/// their own boundary.
///
/// # Errors
///
/// [`RestrictError::RelSchema`] when the target has no relation-ified form,
/// then [`RestrictError::VertexOutsideDomain`] when a row lies at a vertex the
/// migration names only as a target, [`RestrictError::EdgeOutsideDomain`]
/// when a pair lies on an edge the migration names only as a target, or on
/// an unmentioned edge whose relabelling some other edge already reaches (of
/// two unmentioned edges that collide, the later in edge order is named),
/// [`RestrictError::NoEdgeFound`], naming the images of the edge's
/// endpoints, when the migration carries an edge to an image that does not
/// join them (it states no functor), and the error of a row's value
/// transforms (its source table's field transforms, then its op-to-term
/// assignments, applied as the tree extend applies them) when one fails.
/// The output failing validation is [`RestrictError::Validation`] on the
/// output side.
pub fn functor_extend(
    instance: &FInstance,
    tgt_schema: &Schema,
    migration: &CompiledMigration,
    tgt_protocol: &Protocol,
) -> Result<FInstance, RestrictError> {
    let tgt_plan = boundary_plan(tgt_schema, tgt_protocol);
    functor_extend_validated(instance, tgt_schema, migration, tgt_protocol, &tgt_plan)
}

/// [`functor_extend`] with the target's validation plan compiled already.
///
/// # Errors
///
/// [`RestrictError::PlanSchemaMismatch`] on the output side, before
/// anything runs, when `tgt_plan` was compiled for another schema than
/// `tgt_schema`; otherwise as [`functor_extend`].
pub fn functor_extend_validated(
    instance: &FInstance,
    tgt_schema: &Schema,
    migration: &CompiledMigration,
    tgt_protocol: &Protocol,
    tgt_plan: &ValidationPlan,
) -> Result<FInstance, RestrictError> {
    require_schema(
        tgt_plan,
        &crate::identity::schema_digest(tgt_schema),
        BoundarySide::Output,
    )?;
    RelSchema::new(tgt_schema, tgt_protocol)?;
    let extended = functor_extend_engine(instance, migration)?;
    let (budget, external) = unlimited();
    require_passed(
        tgt_plan.validate_table(&extended, &budget, &external),
        BoundarySide::Output,
    )?;
    Ok(extended)
}

/// The extend of a table instance with no classification and no
/// validation: the engine of [`functor_extend`].
///
/// It serves a caller that holds no target schema or protocol (the table
/// lift of panproto-mig), which is why it is public. Rows and pairs move as [`functor_extend`] states.
///
/// # Errors
///
/// [`RestrictError::VertexOutsideDomain`] when a row lies at a vertex the
/// migration names only as a target, [`RestrictError::EdgeOutsideDomain`]
/// when a pair lies on an edge the migration names only as a target, or on
/// an unmentioned edge whose relabelling some other edge already reaches (of
/// two unmentioned edges that collide, the later in edge order is named),
/// [`RestrictError::NoEdgeFound`], naming the images of the edge's
/// endpoints, when the migration carries an edge to an image that does not
/// join them (it states no functor), and the error of a row's value
/// transforms (its source table's field transforms, then its op-to-term
/// assignments, applied as the tree extend applies them) when one fails.
pub fn functor_extend_engine(
    instance: &FInstance,
    migration: &CompiledMigration,
) -> Result<FInstance, RestrictError> {
    crate::adjunction::check_edge_endpoints(migration)?;
    let domain = crate::adjunction::source_vertices(migration);
    let images: std::collections::HashSet<&Name> = migration.vertex_remap.values().collect();
    let mut out = FInstance::new();
    for (src_vertex, table) in instance.tables() {
        if !domain.contains(src_vertex) && images.contains(src_vertex) {
            return Err(RestrictError::VertexOutsideDomain {
                vertex: src_vertex.clone(),
            });
        }
        let tgt_vertex = migration
            .vertex_remap
            .get(src_vertex)
            .cloned()
            .unwrap_or_else(|| src_vertex.clone());
        let transforms = migration.value_transforms(src_vertex);
        for (id, row) in table.rows() {
            let mut new_row = row.clone();
            if !transforms.is_empty() {
                let ctx = TransformContext::for_table(None, instance, id, &transforms);
                let mut node = Node::new(id.0, tgt_vertex.clone());
                node.extra_fields = new_row;
                crate::wtype::apply_field_transforms(&mut node, &transforms, &ctx)?;
                new_row = node.extra_fields;
            }
            out.place_row(tgt_vertex.clone(), id, new_row);
        }
    }

    let tables: Vec<Name> = out.table_names().into_iter().cloned().collect();
    for table in tables {
        let rows = out.rows(&table);
        let ids: Vec<RowId> = rows.iter().map(|(id, _)| *id).collect();
        let columns: BTreeSet<String> = rows
            .iter()
            .flat_map(|(_, row)| row.keys().cloned())
            .collect();
        for id in ids {
            if let Some(row) = out.row_mut(id) {
                for column in &columns {
                    row.entry(column.clone()).or_insert(Value::Null);
                }
            }
        }
    }

    let edge_images = crate::adjunction::edge_images(migration);
    let target_only = crate::adjunction::target_only_edges(migration);
    // Every edge the migration reaches is claimed by the migration itself,
    // whether or not the source edges reaching it hold pairs; an unmentioned
    // edge claims its relabelling only when that edge is still free.
    let mut claimed: HashMap<Edge, Edge> = edge_images
        .iter()
        .map(|(source, image)| (image.clone(), source.clone()))
        .collect();
    for edge in &target_only {
        claimed
            .entry((*edge).clone())
            .or_insert_with(|| (*edge).clone());
    }
    let foreign_keys = instance.foreign_keys();
    let mut carried: Vec<(Edge, &PairSet)> = Vec::with_capacity(foreign_keys.len());
    for (edge, pairs) in foreign_keys {
        let new_edge = match edge_images.get(edge) {
            Some(image) => image.clone(),
            None if target_only.contains(edge) => {
                return Err(RestrictError::EdgeOutsideDomain { edge: edge.clone() });
            }
            None => {
                let relabelled = crate::adjunction::map_edge(migration, edge);
                if claimed.contains_key(&relabelled) {
                    return Err(RestrictError::EdgeOutsideDomain { edge: edge.clone() });
                }
                claimed.insert(relabelled.clone(), edge.clone());
                relabelled
            }
        };
        carried.push((new_edge, pairs));
    }
    for (new_edge, pairs) in carried {
        for &(s, t) in pairs {
            if out.joins(&new_edge, s, t) {
                out.place_pair(new_edge.clone(), s, t);
            }
        }
    }

    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::{BTreeSet, HashMap};

    use super::*;

    /// An edge of kind `kind` named `name` from `src` to `tgt`.
    fn named(src: &str, tgt: &str, kind: &str, name: &str) -> Edge {
        Edge {
            src: src.into(),
            tgt: tgt.into(),
            kind: kind.into(),
            name: Some(name.into()),
        }
    }

    /// Rows `a:0`, `b:1`, `c:2`, with the given pairs.
    fn abc_with(pairs: &[(Edge, (u32, u32))]) -> FInstance {
        let mut inst = FInstance::new()
            .with_rows("a", [(RowId(0), Row::new())])
            .and_then(|f| f.with_rows("b", [(RowId(1), Row::new())]))
            .and_then(|f| f.with_rows("c", [(RowId(2), Row::new())]))
            .unwrap();
        for (edge, (s, t)) in pairs {
            inst = inst
                .with_foreign_key(edge.clone(), [(RowId(*s), RowId(*t))])
                .unwrap();
        }
        inst
    }

    /// `c` merges onto `a`, and `a`, `b` survive under their own names.
    fn c_onto_a() -> CompiledMigration {
        let mut migration = CompiledMigration::default();
        migration.vertex_remap.insert("a".into(), "a".into());
        migration.vertex_remap.insert("c".into(), "a".into());
        migration.vertex_remap.insert("b".into(), "b".into());
        migration.surviving_verts.insert("a".into());
        migration.surviving_verts.insert("b".into());
        migration
    }

    fn refuses_edge(result: Result<FInstance, RestrictError>, expected: &Edge) {
        match result {
            Err(RestrictError::EdgeOutsideDomain { edge }) => assert_eq!(&edge, expected),
            other => panic!("expected EdgeOutsideDomain on {expected:?}, got {other:?}"),
        }
    }

    /// An unmentioned edge relabelled onto the image of a remapped source
    /// edge that holds pairs is refused, not merged into that image.
    #[test]
    fn extend_refuses_an_unmentioned_edge_relabelled_onto_a_remapped_image() {
        let mut migration = c_onto_a();
        migration
            .edge_remap
            .insert(named("a", "b", "fk", "x"), named("a", "b", "fk", "y"));
        migration.surviving_edges.insert(named("a", "b", "fk", "y"));
        let z = named("c", "b", "fk", "y");
        let inst = abc_with(&[(named("a", "b", "fk", "x"), (0, 1)), (z.clone(), (2, 1))]);
        refuses_edge(functor_extend_engine(&inst, &migration), &z);
    }

    /// An unmentioned edge relabelled onto the image of a source edge that
    /// survives under its own name is refused.
    #[test]
    fn extend_refuses_an_unmentioned_edge_relabelled_onto_a_surviving_edge() {
        let mut migration = c_onto_a();
        migration.surviving_edges.insert(named("a", "b", "fk", "w"));
        let z = named("c", "b", "fk", "w");
        let inst = abc_with(&[(named("a", "b", "fk", "w"), (0, 1)), (z.clone(), (2, 1))]);
        refuses_edge(functor_extend_engine(&inst, &migration), &z);
    }

    /// An unmentioned edge relabelled onto a target-only edge is refused
    /// even when the edge remapped onto it holds no pairs: the claim comes
    /// from the migration, not from the instance.
    #[test]
    fn extend_refuses_an_unmentioned_edge_relabelled_onto_a_target_only_edge() {
        let mut migration = c_onto_a();
        migration
            .edge_remap
            .insert(named("a", "b", "fk", "x"), named("a", "b", "fk", "y"));
        let z = named("c", "b", "fk", "y");
        let inst = abc_with(&[(z.clone(), (2, 1))]);
        refuses_edge(functor_extend_engine(&inst, &migration), &z);
    }

    /// Two unmentioned edges whose relabellings coincide are refused: the
    /// later one in edge order is named.
    #[test]
    fn extend_refuses_two_unmentioned_edges_relabelled_onto_one_edge() {
        let migration = c_onto_a();
        let z1 = named("c", "b", "k", "n");
        let z2 = named("a", "b", "k", "n");
        let inst = abc_with(&[(z1.clone(), (2, 1)), (z2, (0, 1))]);
        refuses_edge(functor_extend_engine(&inst, &migration), &z1);
    }

    /// An unmentioned edge whose relabelling is free travels with its pairs
    /// between the images of its endpoints, also when it lands on a source
    /// edge the migration remaps away.
    #[test]
    fn extend_carries_an_unmentioned_edge_onto_a_free_relabelling() {
        let mut migration = c_onto_a();
        migration
            .edge_remap
            .insert(named("a", "b", "fk", "x"), named("a", "b", "fk", "y"));
        let z = named("c", "b", "fk", "free");
        let onto_key = named("c", "b", "fk", "x");
        let inst = abc_with(&[
            (named("a", "b", "fk", "x"), (0, 1)),
            (z, (2, 1)),
            (onto_key, (2, 1)),
        ]);
        let out = functor_extend_engine(&inst, &migration).unwrap();
        assert_eq!(
            out.pairs(&named("a", "b", "fk", "free")),
            &BTreeSet::from([(RowId(2), RowId(1))])
        );
        assert_eq!(
            out.pairs(&named("a", "b", "fk", "y")),
            &BTreeSet::from([(RowId(0), RowId(1))])
        );
        assert_eq!(
            out.pairs(&named("a", "b", "fk", "x")),
            &BTreeSet::from([(RowId(2), RowId(1))])
        );
        assert_eq!(out.pair_count(), 3);
    }

    /// Two source edges that `edge_remap` sends to one image merge, as the
    /// migration states.
    #[test]
    fn extend_merges_source_edges_the_migration_merges() {
        let mut migration = c_onto_a();
        migration
            .edge_remap
            .insert(named("a", "b", "fk", "x"), named("a", "b", "fk", "y"));
        migration
            .edge_remap
            .insert(named("c", "b", "fk", "x2"), named("a", "b", "fk", "y"));
        let inst = abc_with(&[
            (named("a", "b", "fk", "x"), (0, 1)),
            (named("c", "b", "fk", "x2"), (2, 1)),
        ]);
        let out = functor_extend_engine(&inst, &migration).unwrap();
        assert_eq!(
            out.pairs(&named("a", "b", "fk", "y")),
            &BTreeSet::from([(RowId(0), RowId(1)), (RowId(2), RowId(1))])
        );
    }

    /// Extend merges two foreign keys only when the migration says so.
    mod extend_merges_only_what_the_migration_states {
        use proptest::prelude::*;

        use super::*;

        const VERTS: [&str; 4] = ["a", "b", "c", "d"];
        const NAMES: [&str; 2] = ["p", "q"];

        /// The edge with index `i` of the 32-edge universe over [`VERTS`]
        /// and [`NAMES`].
        fn universe(i: usize) -> Edge {
            Edge {
                src: VERTS[i % 4].into(),
                tgt: VERTS[(i / 4) % 4].into(),
                kind: "fk".into(),
                name: Some(NAMES[(i / 16) % 2].into()),
            }
        }

        prop_compose! {
            fn migration()(
                vertex_map in prop::collection::vec(prop::option::of(0usize..4), 4),
                fix_images in any::<bool>(),
                surviving in prop::collection::vec(any::<bool>(), 4),
                edge_map in prop::collection::vec((0usize..32, 0usize..32), 0..4),
                surviving_edges in prop::collection::vec(0usize..32, 0..4),
            ) -> CompiledMigration {
                let mut migration = CompiledMigration::default();
                for (v, image) in vertex_map.iter().enumerate() {
                    if let Some(t) = image {
                        migration.vertex_remap.insert(VERTS[v].into(), VERTS[*t].into());
                    }
                }
                if fix_images {
                    let images: Vec<Name> = migration.vertex_remap.values().cloned().collect();
                    for image in images {
                        migration.vertex_remap.entry(image.clone()).or_insert(image);
                    }
                }
                for (v, keep) in surviving.iter().enumerate() {
                    if *keep {
                        migration.surviving_verts.insert(VERTS[v].into());
                    }
                }
                for (from, to) in edge_map {
                    migration.edge_remap.insert(universe(from), universe(to));
                }
                for e in surviving_edges {
                    migration.surviving_edges.insert(universe(e));
                }
                migration
            }
        }

        /// One row per vertex (`RowId(i)` at `VERTS[i]`), and the pair of
        /// the endpoint rows on each chosen edge.
        fn instance(edges: &[usize]) -> FInstance {
            let mut inst = FInstance::new();
            for (i, v) in VERTS.iter().enumerate() {
                inst = inst
                    .with_rows(*v, [(RowId(u32::try_from(i).unwrap()), Row::new())])
                    .unwrap();
            }
            for &i in edges {
                let e = universe(i);
                let s = u32::try_from(VERTS.iter().position(|v| e.src == *v).unwrap()).unwrap();
                let t = u32::try_from(VERTS.iter().position(|v| e.tgt == *v).unwrap()).unwrap();
                inst = inst.with_foreign_key(e, [(RowId(s), RowId(t))]).unwrap();
            }
            inst
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(1024))]

            #[test]
            fn extend_merges_only_source_edges(
                migration in migration(),
                edges in prop::collection::btree_set(0usize..32, 0..8),
            ) {
                let edges: Vec<usize> = edges.into_iter().collect();
                let inst = instance(&edges);
                let images = crate::adjunction::edge_images(&migration);
                let target_only = crate::adjunction::target_only_edges(&migration);
                // Every target-only edge is the image of the edge remapped
                // onto it, so it is also an image value.
                for e in &target_only {
                    prop_assert!(images.values().any(|image| image == *e));
                }
                let Ok(out) = functor_extend_engine(&inst, &migration) else {
                    return Ok(());
                };
                // Which input edges sent pairs to each output edge.
                let mut contributors: HashMap<Edge, Vec<Edge>> = HashMap::new();
                for (edge, _) in inst.foreign_keys() {
                    prop_assert!(!target_only.contains(edge));
                    let carried = images
                        .get(edge)
                        .cloned()
                        .unwrap_or_else(|| crate::adjunction::map_edge(&migration, edge));
                    contributors.entry(carried).or_default().push(edge.clone());
                }
                for (edge, pairs) in out.foreign_keys() {
                    let from = contributors.get(edge).cloned().unwrap_or_default();
                    prop_assert!(!from.is_empty(), "{edge:?} has pairs but no preimage");
                    let mut expected = BTreeSet::new();
                    for source in &from {
                        expected.extend(inst.pairs(source).iter().copied());
                    }
                    prop_assert!(pairs.is_subset(&expected));
                    let unmentioned: Vec<&Edge> =
                        from.iter().filter(|e| !images.contains_key(*e)).collect();
                    if !unmentioned.is_empty() {
                        prop_assert_eq!(
                            from.len(), 1,
                            "{:?} merges {:?} unasked", edge, from
                        );
                        prop_assert!(
                            !images.values().any(|image| image == edge),
                            "unmentioned {:?} lands on the image {:?}", unmentioned, edge
                        );
                    }
                }
            }
        }
    }
}
