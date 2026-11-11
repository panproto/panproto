//! Table edit lenses: a table lens bound to a source table that translates
//! table deltas in both directions.
//!
//! A [`TableEditLens`] is the table shape of
//! [`ShapeEditLens`]. Its forward step applies a
//! source [`TableDelta`], runs table get on the edited source under the
//! boundary protocols of its [`TableContext`], and returns the
//! incidence-closed replacement differ from the held view to that view
//! ([`TableDelta::diff_keeping_replacements`]). Rows keep their source ids
//! and a pair is named by its foreign key and its two rows, so no element of
//! a table view carries a synthesized id and the presentation of the view
//! is that of whole-state get. A step runs table get on the whole edited
//! source, so its work is linear in the size of the table.
//!
//! On the table-shape domain of the lens page, the graph and table routes
//! of a lens commute, and so do their edit lenses: the table edit lens's
//! view delta is the table reading of the graph edit lens's view delta on
//! the graph form of the same edit.

use panproto_inst::graph_edit::ComposeError;
use panproto_inst::identity::{ComplementShape, Digest, IdentityCarrier};
use panproto_inst::table_delta::TableDelta;
use panproto_inst::{
    EditError, FInstance, RestrictPlan, TableComplement, functor_restrict_finite_with_plan,
};

use crate::shape_edit_lens::{
    EditShape, ShapeEditError, ShapeEditLens, ShapeEditStep, get_refusal, incoherent,
};
use crate::table_shape::{TableContext, TableShape};
use crate::{Lens, LensError, table_lens};

/// The state a [`TableEditLens`] holds: the source, its view and complement,
/// and the compiled table plan.
#[derive(Clone, Debug)]
pub struct TableHeld {
    plan: RestrictPlan,
    source: FInstance,
    view: FInstance,
    complement: TableComplement,
}

/// A translated table delta, ready to commit: the source already holds the
/// edited state, and `undo` restores it.
pub struct TableStaged {
    undo: TableDelta,
    view_delta: TableDelta,
    complement: TableComplement,
}

/// A table lens bound to a source table, translating table deltas in both
/// directions.
///
/// ```
/// use std::collections::{BTreeSet, HashMap};
///
/// use panproto_inst::table_delta::TableDelta;
/// use panproto_inst::{CompiledMigration, FInstance, RowId, Value};
/// use panproto_lens::{Lens, TableContext, TableEditLens};
/// use panproto_schema::{Protocol, SchemaBuilder};
///
/// let protocol = Protocol {
///     non_value_kinds: BTreeSet::from(["record".to_owned()]),
///     ..Protocol::default()
/// };
/// let schema = SchemaBuilder::new(&protocol)
///     .vertex("user", "record", None)
///     .unwrap()
///     .build()
///     .unwrap();
/// let compiled = CompiledMigration {
///     surviving_verts: schema.vertices.keys().cloned().collect(),
///     ..CompiledMigration::default()
/// };
/// let lens = Lens { compiled, src_schema: schema.clone(), tgt_schema: schema };
/// let context = TableContext { src: protocol.clone(), tgt: protocol };
/// let source = FInstance::new().with_rows("user", [(RowId(0), HashMap::new())]).unwrap();
/// let mut edit_lens = TableEditLens::new(lens, source, context).unwrap();
///
/// let add = TableDelta::identity().with_added_row(RowId(1), "user", HashMap::new());
/// let step = edit_lens.get_edit(&add).unwrap();
/// assert_eq!(step.view_delta, add);
/// assert!(edit_lens.view().row(RowId(1)).is_some());
/// ```
pub type TableEditLens = ShapeEditLens<TableShape>;

/// An admitted table edit with its translation.
pub type TableEditStep = ShapeEditStep<TableDelta>;

impl ShapeEditLens<TableShape> {
    /// Bind `lens` to `source` under the boundary protocols of `context`:
    /// run table get on it and hold the source, the view and the stamped
    /// complement.
    ///
    /// # Errors
    ///
    /// [`ShapeEditError::Get`] with the refusal of table get on `source`.
    pub fn new(
        lens: Lens,
        source: FInstance,
        context: TableContext,
    ) -> Result<Self, ShapeEditError> {
        Self::bind(lens, context, source)
    }
}

/// Table get on `source` along `plan`, stamped with the table identity of
/// `lens`.
fn table_get(
    lens: &Lens,
    plan: &RestrictPlan,
    source: &FInstance,
) -> Result<(FInstance, TableComplement), ShapeEditError> {
    let (view, mut complement) =
        functor_restrict_finite_with_plan(source, plan).map_err(get_refusal)?;
    complement.stamp(lens.identity(ComplementShape::Functor), &lens.src_schema);
    Ok((view, complement))
}

impl EditShape for TableShape {
    type Instance = FInstance;
    type Complement = TableComplement;
    type Delta = TableDelta;
    type Context = TableContext;
    type Held = TableHeld;
    type Staged = TableStaged;

    fn bind(
        lens: &Lens,
        context: &TableContext,
        source: FInstance,
    ) -> Result<TableHeld, ShapeEditError> {
        let plan = RestrictPlan::compile_for_tables(
            &lens.src_schema,
            &lens.tgt_schema,
            &lens.compiled,
            context.protocols(),
        )
        .map_err(get_refusal)?;
        let (view, complement) = table_get(lens, &plan, &source)?;
        Ok(TableHeld {
            plan,
            source,
            view,
            complement,
        })
    }

    fn source(held: &TableHeld) -> &FInstance {
        &held.source
    }

    fn view(held: &TableHeld) -> &FInstance {
        &held.view
    }

    fn complement(held: &TableHeld) -> &TableComplement {
        &held.complement
    }

    fn stage(
        lens: &Lens,
        _context: &TableContext,
        held: &mut TableHeld,
        delta: &TableDelta,
    ) -> Result<TableStaged, ShapeEditError> {
        delta
            .apply(&mut held.source)
            .map_err(ShapeEditError::Apply)?;
        let undo = delta.invert();
        match table_get(lens, &held.plan, &held.source) {
            Ok((view, complement)) => Ok(TableStaged {
                undo,
                view_delta: TableDelta::diff_keeping_replacements(&held.view, &view, delta),
                complement,
            }),
            Err(error) => {
                undo.apply(&mut held.source)
                    .map_err(|undo_error| incoherent(undo_error.to_string()))?;
                Err(error)
            }
        }
    }

    fn staged_view_delta(staged: &TableStaged) -> &TableDelta {
        &staged.view_delta
    }

    fn abort(held: &mut TableHeld, staged: &TableStaged) -> Result<(), ShapeEditError> {
        staged
            .undo
            .apply(&mut held.source)
            .map_err(|error| incoherent(error.to_string()))
    }

    fn commit(held: &mut TableHeld, staged: TableStaged) -> Result<(), ShapeEditError> {
        if let Err(error) = staged.view_delta.apply(&mut held.view) {
            Self::abort(held, &staged)?;
            return Err(incoherent(format!(
                "the translated view delta does not apply to the held view: {error}"
            )));
        }
        held.complement = staged.complement;
        Ok(())
    }

    fn apply(delta: &TableDelta, instance: &mut FInstance) -> Result<(), EditError> {
        delta.apply(instance)
    }

    fn identity() -> TableDelta {
        TableDelta::identity()
    }

    fn compose(first: &TableDelta, next: &TableDelta) -> Result<TableDelta, ComposeError> {
        first.compose(next)
    }

    fn digest(delta: &TableDelta) -> Digest {
        delta.digest()
    }

    fn identical(a: &FInstance, b: &FInstance) -> bool {
        TableDelta::diff(a, b).is_identity()
    }

    fn closed_diff(
        before: &FInstance,
        after: &FInstance,
        input: &TableDelta,
    ) -> Result<TableDelta, ShapeEditError> {
        Ok(TableDelta::diff_keeping_replacements(before, after, input))
    }

    fn put(
        lens: &Lens,
        context: &TableContext,
        view: &FInstance,
        complement: &TableComplement,
    ) -> Result<FInstance, LensError> {
        table_lens::put(lens, view, complement, context.protocols())
    }
}
