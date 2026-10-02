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
/// with no renumbering, and the operation creates no rows. Within each
/// target table the rows are padded to the union of their columns with
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
/// join them (it states no functor), and `RestrictError` if a row's
/// op-to-term assignment fails to evaluate.
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
/// It serves a caller that holds no target schema or protocol. Rows and
/// pairs move as [`functor_extend`] states.
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
/// join them (it states no functor), and `RestrictError` if a row's
/// op-to-term assignment fails to evaluate.
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
        let assignments = migration.op_term_assignments.get(src_vertex);
        for (id, row) in table.rows() {
            let mut new_row = row.clone();
            if let Some(assignments) = assignments {
                crate::wtype::apply_term_assignments_to_row(&mut new_row, assignments)?;
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
