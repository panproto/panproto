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
use panproto_schema::{Edge, Schema};

use crate::adjunction::NonConformance;
use crate::contraction_plan::PairResolution;
use crate::error::RestrictError;
use crate::functor::{FInstance, Row, same_value};
use crate::metadata::Node;
use crate::rel_schema::RelSchema;
use crate::restrict_boundary::{
    BoundaryProtocols, BoundarySide, boundary_plan, output_plan, require_passed, require_schema,
    unlimited,
};
use crate::restrict_plan::{EdgeFate, RestrictPlan, ReverseAnchors, floor_above};
use crate::table_complement::{DroppedRow, TableComplement};
use crate::validation::ValidationPlan;
use crate::view_ref::RowId;
use crate::wtype::{CompiledMigration, InstanceRef, TransformContext};

/// The restrict of a table instance along a plan compiled for tables: the
/// single table engine.
///
/// The precondition is the caller's: this function validates nothing
/// beyond the shape of each row, so the instance should be valid against
/// the plan's source schema ([`functor_restrict_with_complement`] validates
/// both boundaries). Every row must be total, a cell at each column of its
/// table and at no other key, which it refuses otherwise: on total rows get
/// paired with the complement is injective. The complement is unstamped;
/// the lens stamps it.
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
/// table functor. Then, in ascending row id and for each row in this order,
/// [`RestrictError::NonConforming`] for a row at no table of the source
/// schema or whose cells are not exactly its table's columns (the engine
/// reads total rows: an absent cell and a [`crate::Value::Null`] one differ), and
/// the refusals of its survival predicate and transforms; in ascending foreign key and pair,
/// [`RestrictError::IncoherentEdgeImage`] for a pair whose key is no source
/// schema edge, and [`RestrictError::NotTableRepresentable`],
/// [`RestrictError::NoEdgeFound`] or [`RestrictError::AmbiguousEdge`] for a
/// key whose fate cannot be a pair; and, in ascending pair of rows, the same
/// for a contracted pair. The table engine locates no error.
pub fn functor_restrict_with_plan(
    instance: &FInstance,
    plan: &RestrictPlan,
) -> Result<(FInstance, TableComplement), RestrictError> {
    let Some(functor) = plan.table_functor() else {
        return Err(RestrictError::TablePlanRequired);
    };
    let mut run = TableRun::new(instance, plan, functor.source());
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
    /// The source schema's tables and their columns.
    source: &'a RelSchema,
    complement: TableComplement,
    view: FInstance,
    /// For each view pair, the source keys that carried it.
    carriers: BTreeMap<(Edge, RowId, RowId), BTreeSet<Edge>>,
}

impl<'a> TableRun<'a> {
    fn new(instance: &'a FInstance, plan: &'a RestrictPlan, source: &'a RelSchema) -> Self {
        let mut complement = TableComplement::empty();
        complement.id_floor = floor_above(instance.max_row_id().map(|r| r.0));
        Self {
            instance,
            plan,
            migration: plan.migration(),
            reverse: ReverseAnchors::new(plan.migration()),
            source,
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
            self.conform(id, table, row)?;
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

    /// Refuse a row that lies at no table of the source schema, or whose
    /// cells are not exactly its table's columns, at the least column that
    /// is missing or stray.
    ///
    /// The complement names the source columns on which a view row and its
    /// source row disagree, which determines the source row only when its
    /// keys are fixed: `AddField c := 0` sends `{a: 1}` and `{a: 1, c: 0}`
    /// to one view row, and leaves the second nothing to record.
    fn conform(&self, id: RowId, table: &Name, row: &Row) -> Result<(), RestrictError> {
        let refuse = |column: Option<String>, reason: NonConformance| {
            Err(RestrictError::NonConforming {
                row: id,
                column,
                reason,
            })
        };
        if !self.source.is_table(table) {
            return refuse(
                None,
                NonConformance::TableOutside {
                    table: table.clone(),
                },
            );
        }
        let columns = self.source.columns(table);
        let missing = columns.keys().find(|column| !row.contains_key(*column));
        let stray = row.keys().filter(|key| !columns.contains_key(*key)).min();
        match (missing, stray) {
            (Some(missing), Some(stray)) if stray < missing => {
                refuse(Some(stray.clone()), NonConformance::StrayCell)
            }
            (Some(missing), _) => refuse(Some(missing.clone()), NonConformance::MissingCell),
            (None, Some(stray)) => refuse(Some(stray.clone()), NonConformance::StrayCell),
            (None, None) => Ok(()),
        }
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
    let tgt_plan = output_plan(&src_plan, tgt_schema, &plan.tgt_schema, protocols);
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
/// target schema under `protocols.tgt` and validating both boundaries.
///
/// This is the left Kan extension: given an instance of the source schema
/// and a migration mapping (source -> target), produce an instance of the
/// target schema by moving every table to its image. Every realized row,
/// column and relation pair must have an image. Rows keep their [`RowId`]s;
/// thus a non-injective vertex map forms a disjoint union without
/// renumbering. Equal relation pairs coalesce because a table relation is a
/// set. Columns follow the table functor, with `Sigma_F` introducing labelled
/// nulls for target columns outside the image and solving the equations of a
/// non-injective column map. Field transforms run before that extension.
///
/// Refusing an element with no image follows the explicit validation pattern
/// of CQL `Mapping.validate`; related boundary precedents include Avro's
/// `avrospec`, Protocol Buffers' `protobufprotojson`, EMF `XMLResource`, and
/// the DPO dangling condition. Deletion belongs to the partial form.
/// Skolemization, labelled nulls, and preservation as unknown are alternative
/// boundary policies; none makes refusal a law uniformly across every shape.
///
/// # Errors
///
/// [`RestrictError::RelSchema`] when either schema has no relation-ified
/// form, [`RestrictError::UnmappedAnchor`] or
/// [`RestrictError::UnmappedEdge`] for a realized element with no image, a
/// `Sigma_F` equation error, a transform error, or
/// [`RestrictError::Validation`] when either boundary does not validate.
pub fn functor_extend(
    instance: &FInstance,
    src_schema: &Schema,
    tgt_schema: &Schema,
    migration: &CompiledMigration,
    protocols: BoundaryProtocols<'_>,
) -> Result<FInstance, RestrictError> {
    let total = crate::wtype::total_extension_migration(src_schema, tgt_schema, migration);
    require_total_table_elements(instance, tgt_schema, &total)?;
    let src_validation = boundary_plan(src_schema, protocols.src);
    let plan =
        RestrictPlan::compile_for_table_extension(src_schema, tgt_schema, &total, protocols)?;
    let tgt_validation = output_plan(&src_validation, tgt_schema, &plan.tgt_schema, protocols);
    functor_extend_validated(instance, &src_validation, &tgt_validation, &plan)
}

/// [`functor_extend`] with both boundary validation plans and the migration
/// plan compiled already.
///
/// # Errors
///
/// [`RestrictError::PlanSchemaMismatch`] on the output side, before
/// anything runs, when `tgt_plan` was compiled for another schema than
/// `tgt_schema`; otherwise as [`functor_extend`].
pub fn functor_extend_validated(
    instance: &FInstance,
    src_plan: &ValidationPlan,
    tgt_plan: &ValidationPlan,
    plan: &RestrictPlan,
) -> Result<FInstance, RestrictError> {
    require_schema(src_plan, &plan.src_schema, BoundarySide::Input)?;
    require_schema(tgt_plan, &plan.tgt_schema, BoundarySide::Output)?;
    let (budget, external) = unlimited();
    require_passed(
        src_plan.validate_table(instance, &budget, &external),
        BoundarySide::Input,
    )?;
    let extended = functor_extend_inner(instance, plan, false)?.0;
    require_passed(
        tgt_plan.validate_table(&extended, &budget, &external),
        BoundarySide::Output,
    )?;
    Ok(extended)
}

/// The extend of a table instance with no classification and no
/// validation: the engine of [`functor_extend`].
///
/// It is public for callers such as the table lift in `panproto-mig`. Rows
/// and pairs move as [`functor_extend`] states.
///
/// # Errors
///
/// As [`functor_extend`], except boundary validation errors.
pub fn functor_extend_engine(
    instance: &FInstance,
    src_schema: &Schema,
    tgt_schema: &Schema,
    migration: &CompiledMigration,
    protocols: BoundaryProtocols<'_>,
) -> Result<FInstance, RestrictError> {
    let total = crate::wtype::total_extension_migration(src_schema, tgt_schema, migration);
    require_total_table_elements(instance, tgt_schema, &total)?;
    let plan =
        RestrictPlan::compile_for_table_extension(src_schema, tgt_schema, &total, protocols)?;
    functor_extend_inner(instance, &plan, false).map(|(extended, _)| extended)
}

/// Partial table extension: omit every unmapped row, cell, or pair and
/// return the elements at which the omissions occurred.
///
/// # Errors
///
/// Plan compilation, Sigma, transform, or boundary validation errors.
pub fn functor_extend_partial(
    instance: &FInstance,
    src_schema: &Schema,
    tgt_schema: &Schema,
    migration: &CompiledMigration,
    protocols: BoundaryProtocols<'_>,
) -> Result<(FInstance, Vec<crate::ViewRef>), RestrictError> {
    let total = crate::wtype::total_extension_migration(src_schema, tgt_schema, migration);
    let src_validation = boundary_plan(src_schema, protocols.src);
    let plan =
        RestrictPlan::compile_for_table_extension(src_schema, tgt_schema, &total, protocols)?;
    let tgt_validation = output_plan(&src_validation, tgt_schema, &plan.tgt_schema, protocols);
    let (budget, external) = unlimited();
    require_passed(
        src_validation.validate_table(instance, &budget, &external),
        BoundarySide::Input,
    )?;
    let result = functor_extend_inner(instance, &plan, true)?;
    require_passed(
        tgt_validation.validate_table(&result.0, &budget, &external),
        BoundarySide::Output,
    )?;
    Ok(result)
}

fn sigma_error(error: crate::AdjunctionError) -> RestrictError {
    match error {
        crate::AdjunctionError::Sigma(error) | crate::AdjunctionError::Pi(error) => error,
        crate::AdjunctionError::SigmaConstantClash {
            row,
            column,
            left,
            right,
        } => RestrictError::SigmaConstantClash {
            row,
            column,
            left,
            right,
        },
        crate::AdjunctionError::NullIdsExhausted => RestrictError::NullIdsExhausted,
        crate::AdjunctionError::NonConforming {
            row,
            column,
            reason,
        } => RestrictError::NonConforming {
            row,
            column,
            reason,
        },
        other => RestrictError::NotTableRepresentable {
            reason: other.to_string(),
        },
    }
}

fn require_total_table_elements(
    instance: &FInstance,
    target: &Schema,
    migration: &CompiledMigration,
) -> Result<(), RestrictError> {
    for id in instance.row_ids() {
        let Some(table) = instance.table_of(id) else {
            continue;
        };
        if crate::wtype::extension_vertex_image(target, migration, table).is_none() {
            return Err(RestrictError::UnmappedAnchor {
                anchor: table.clone(),
                node_id: id.0,
            });
        }
    }
    for (edge, pairs) in instance.foreign_keys() {
        if pairs.is_empty() || crate::wtype::extension_edge_image(target, migration, edge).is_some()
        {
            continue;
        }
        let Some(&(src, tgt)) = pairs.iter().next() else {
            continue;
        };
        return Err(RestrictError::UnmappedEdge {
            edge: Box::new(edge.clone()),
            element: crate::ViewRef::Pair {
                edge: edge.clone(),
                src,
                tgt,
            },
        });
    }
    Ok(())
}

fn functor_extend_inner(
    instance: &FInstance,
    plan: &RestrictPlan,
    partial: bool,
) -> Result<(FInstance, Vec<crate::ViewRef>), RestrictError> {
    let entity = plan
        .table_functor()
        .ok_or(RestrictError::TablePlanRequired)?;
    let functor = entity.clone().into_kan()?;
    let mut dropped = Vec::new();

    for id in instance.row_ids() {
        let Some(table) = instance.table_of(id) else {
            continue;
        };
        if !functor.table_map().contains_key(table) {
            if partial {
                dropped.push(crate::ViewRef::Row(id));
                continue;
            }
            return Err(RestrictError::UnmappedAnchor {
                anchor: table.clone(),
                node_id: id.0,
            });
        }
        for edge in functor.source().columns(table).values() {
            let Some(column) = functor.source().attribute_column(edge) else {
                continue;
            };
            if functor.column_image(table.as_str(), column).is_none() {
                let element = crate::ViewRef::Row(id);
                if partial {
                    dropped.push(element);
                    continue;
                }
                return Err(RestrictError::UnmappedEdge {
                    edge: Box::new(edge.clone()),
                    element,
                });
            }
        }
    }
    for (edge, pairs) in instance.foreign_keys() {
        if functor.relation_map().contains_key(edge) {
            continue;
        }
        for &(src, tgt) in pairs {
            let element = crate::ViewRef::Pair {
                edge: edge.clone(),
                src,
                tgt,
            };
            if partial {
                dropped.push(element);
            } else {
                return Err(RestrictError::UnmappedEdge {
                    edge: Box::new(edge.clone()),
                    element,
                });
            }
        }
    }

    let mut out = if partial {
        let kept = crate::f_restrict_to_domain(instance, &functor).map_err(sigma_error)?;
        crate::f_sigma_restricted(&kept, &functor)
            .map(|(extended, _)| extended)
            .map_err(sigma_error)?
    } else {
        crate::f_sigma(instance, &functor).map_err(sigma_error)?
    };
    let migration = plan.migration();
    for id in out.row_ids() {
        let Some(source_table) = instance.table_of(id) else {
            continue;
        };
        let transforms = migration.value_transforms(source_table);
        if transforms.is_empty() {
            continue;
        }
        let Some(image_table) = out.table_of(id).cloned() else {
            continue;
        };
        let Some(cells) = out.row(id).cloned() else {
            continue;
        };
        let ctx = TransformContext::for_table(Some(plan.source()), instance, id, &transforms);
        let mut node = Node::new(id.0, image_table);
        node.extra_fields = cells;
        plan.transform(&mut node, source_table, &ctx, false)?;
        if let Some(row) = out.row_mut(id) {
            *row = node.extra_fields;
        }
    }
    dropped.sort_unstable();
    dropped.dedup();
    Ok((out, dropped))
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashMap};

    use super::*;

    /// The result of a test whose setup can fail.
    type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

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
    fn abc_with(pairs: &[(Edge, (u32, u32))]) -> TestResult<FInstance> {
        let mut inst = FInstance::new()
            .with_rows("a", [(RowId(0), Row::new())])?
            .with_rows("b", [(RowId(1), Row::new())])?
            .with_rows("c", [(RowId(2), Row::new())])?;
        for (edge, (s, t)) in pairs {
            inst = inst.with_foreign_key(edge.clone(), [(RowId(*s), RowId(*t))])?;
        }
        Ok(inst)
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

    fn table_schema(vertices: BTreeSet<Name>, edges: BTreeSet<Edge>) -> Schema {
        let mut outgoing = HashMap::new();
        let mut incoming = HashMap::new();
        let mut between = HashMap::new();
        for edge in &edges {
            outgoing
                .entry(edge.src.clone())
                .or_insert_with(smallvec::SmallVec::new)
                .push(edge.clone());
            incoming
                .entry(edge.tgt.clone())
                .or_insert_with(smallvec::SmallVec::new)
                .push(edge.clone());
            between
                .entry((edge.src.clone(), edge.tgt.clone()))
                .or_insert_with(smallvec::SmallVec::new)
                .push(edge.clone());
        }
        Schema {
            protocol: "test".into(),
            vertices: vertices
                .into_iter()
                .map(|id| {
                    (
                        id.clone(),
                        panproto_schema::Vertex {
                            id,
                            kind: "record".into(),
                            nsid: None,
                        },
                    )
                })
                .collect(),
            edges: edges
                .into_iter()
                .map(|edge| (edge.clone(), edge.kind))
                .collect(),
            outgoing,
            incoming,
            between,
            ..Schema::default()
        }
    }

    fn extend_for_test(
        instance: &FInstance,
        migration: &CompiledMigration,
    ) -> Result<FInstance, RestrictError> {
        let src_vertices: BTreeSet<Name> = instance.table_names().into_iter().cloned().collect();
        let src_edges: BTreeSet<Edge> = instance
            .foreign_keys()
            .into_iter()
            .map(|(edge, _)| edge.clone())
            .collect();
        let src = table_schema(src_vertices.clone(), src_edges.clone());

        let mut tgt_vertices: BTreeSet<Name> = src_vertices
            .iter()
            .map(|vertex| {
                migration
                    .vertex_remap
                    .get(vertex)
                    .cloned()
                    .unwrap_or_else(|| vertex.clone())
            })
            .collect();
        tgt_vertices.extend(migration.surviving_verts.iter().cloned());
        tgt_vertices.extend(migration.vertex_remap.values().cloned());
        let mut tgt_edges: BTreeSet<Edge> = src_edges
            .iter()
            .map(|edge| crate::adjunction::map_edge(migration, edge))
            .collect();
        tgt_edges.extend(migration.surviving_edges.iter().cloned());
        tgt_edges.extend(migration.edge_remap.values().cloned());
        let tgt = table_schema(tgt_vertices, tgt_edges);

        let protocol = panproto_schema::Protocol {
            non_value_kinds: BTreeSet::from(["record".to_owned()]),
            ..panproto_schema::Protocol::default()
        };
        functor_extend_engine(
            instance,
            &src,
            &tgt,
            migration,
            BoundaryProtocols {
                src: &protocol,
                tgt: &protocol,
            },
        )
    }

    /// A relation whose structural image equals an explicit image coalesces
    /// with it, as table foreign keys are sets of pairs.
    #[test]
    fn extend_coalesces_an_implicit_and_explicit_relation_image() -> TestResult {
        let mut migration = c_onto_a();
        migration
            .edge_remap
            .insert(named("a", "b", "fk", "x"), named("a", "b", "fk", "y"));
        migration.surviving_edges.insert(named("a", "b", "fk", "y"));
        let z = named("c", "b", "fk", "y");
        let inst = abc_with(&[(named("a", "b", "fk", "x"), (0, 1)), (z, (2, 1))])?;
        let out = extend_for_test(&inst, &migration)?;
        assert_eq!(
            out.pairs(&named("a", "b", "fk", "y")),
            &BTreeSet::from([(RowId(0), RowId(1)), (RowId(2), RowId(1))])
        );
        Ok(())
    }

    /// The same coalescence applies to a relation surviving under its name.
    #[test]
    fn extend_coalesces_onto_a_surviving_relation() -> TestResult {
        let mut migration = c_onto_a();
        migration.surviving_edges.insert(named("a", "b", "fk", "w"));
        let z = named("c", "b", "fk", "w");
        let inst = abc_with(&[(named("a", "b", "fk", "w"), (0, 1)), (z, (2, 1))])?;
        let out = extend_for_test(&inst, &migration)?;
        assert_eq!(
            out.pairs(&named("a", "b", "fk", "w")),
            &BTreeSet::from([(RowId(0), RowId(1)), (RowId(2), RowId(1))])
        );
        Ok(())
    }

    /// An implicit relation uses the unique direct target relation even when
    /// that relation is also an explicit image.
    #[test]
    fn extend_uses_a_unique_direct_target_relation() -> TestResult {
        let mut migration = c_onto_a();
        migration
            .edge_remap
            .insert(named("a", "b", "fk", "x"), named("a", "b", "fk", "y"));
        let z = named("c", "b", "fk", "y");
        let inst = abc_with(&[(z, (2, 1))])?;
        let out = extend_for_test(&inst, &migration)?;
        assert_eq!(
            out.pairs(&named("a", "b", "fk", "y")),
            &BTreeSet::from([(RowId(2), RowId(1))])
        );
        Ok(())
    }

    /// Two implicit relations with the same structural image coalesce.
    #[test]
    fn extend_coalesces_two_implicit_relation_images() -> TestResult {
        let migration = c_onto_a();
        let z1 = named("c", "b", "k", "n");
        let z2 = named("a", "b", "k", "n");
        let inst = abc_with(&[(z1, (2, 1)), (z2, (0, 1))])?;
        let out = extend_for_test(&inst, &migration)?;
        assert_eq!(
            out.pairs(&named("a", "b", "k", "n")),
            &BTreeSet::from([(RowId(0), RowId(1)), (RowId(2), RowId(1))])
        );
        Ok(())
    }

    /// An implicit edge is refused when several direct target edges join the
    /// endpoint images, because its image is not uniquely determined.
    #[test]
    fn extend_refuses_an_ambiguous_direct_relation_image() -> TestResult {
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
        ])?;
        let Err(error) = extend_for_test(&inst, &migration) else {
            return Err("ambiguous relation image unexpectedly extended".into());
        };
        assert!(
            matches!(
                error,
                RestrictError::UnmappedEdge { ref edge, .. }
                    if edge.as_ref() == &named("c", "b", "fk", "free")
            ),
            "{error:?}"
        );
        Ok(())
    }

    /// Two source edges that `edge_remap` sends to one image merge, as the
    /// migration states.
    #[test]
    fn extend_merges_source_edges_the_migration_merges() -> TestResult {
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
        ])?;
        let out = extend_for_test(&inst, &migration)?;
        assert_eq!(
            out.pairs(&named("a", "b", "fk", "y")),
            &BTreeSet::from([(RowId(0), RowId(1)), (RowId(2), RowId(1))])
        );
        Ok(())
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
        fn instance(edges: &[usize]) -> TestResult<FInstance> {
            let row_of = |vertex: &Name| -> TestResult<RowId> {
                let index = VERTS
                    .iter()
                    .position(|v| vertex == *v)
                    .ok_or("an edge of the universe ends outside VERTS")?;
                Ok(RowId(u32::try_from(index)?))
            };
            let mut inst = FInstance::new();
            for (i, v) in VERTS.iter().enumerate() {
                inst = inst.with_rows(*v, [(RowId(u32::try_from(i)?), Row::new())])?;
            }
            for &i in edges {
                let e = universe(i);
                let (s, t) = (row_of(&e.src)?, row_of(&e.tgt)?);
                inst = inst.with_foreign_key(e, [(s, t)])?;
            }
            Ok(inst)
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(1024))]

            #[test]
            fn extend_merges_only_source_edges(
                migration in migration(),
                edges in prop::collection::btree_set(0usize..32, 0..8),
            ) {
                let edges: Vec<usize> = edges.into_iter().collect();
                let inst = instance(&edges)
                    .map_err(|e| TestCaseError::fail(e.to_string()))?;
                let Ok(out) = extend_for_test(&inst, &migration) else {
                    return Ok(());
                };
                let input_pairs: BTreeSet<(RowId, RowId)> = inst
                    .foreign_keys()
                    .into_iter()
                    .flat_map(|(_, pairs)| pairs.iter().copied())
                    .collect();
                for (_, pairs) in out.foreign_keys() {
                    prop_assert!(pairs.is_subset(&input_pairs));
                }
            }
        }
    }
}
