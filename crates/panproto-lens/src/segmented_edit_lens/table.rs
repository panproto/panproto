//! Table edit lenses over a table held as segments.
//!
//! A [`SegmentedTableEditLens`] is the [`ShapeEditLens`] of the segmented
//! table shape ([`SegmentedTableShape`]). It binds a sealed source family
//! ([`SegmentedTable`]) by running segmented table get over it
//! ([`TableRestrict`]), and holds the source manifest, the view manifest of
//! that get and the run whose Emit artifacts hold the view and complement
//! segments. No step assembles the source or the view.
//!
//! [`ShapeEditLens::get_edit`] translates a source [`TableDelta`] in three
//! runs against the store: the delta applied to the source family as a
//! checkpointed edit ([`apply_run`]), which reads and rewrites only the
//! segments it can change; the edited family restricted again, carrying
//! tasks over from the held get ([`RunSpec::reuse`]) so only the segments
//! whose records changed are read again; and the view delta, the
//! incidence-closed replacement differ between the held view and the new
//! view keeping the replacements of the source delta, computed over the
//! view segments whose records changed, the segments owning an element the
//! delta replaces, and the segments their boundaries name. A table view
//! carries no synthesized identifier (a contracted pair is named by its key
//! and rows), so the view delta is the whole-state table edit lens's.
//!
//! [`ShapeEditLens::put_edit`] applies a view delta to the view family,
//! puts the edited view back segment by segment
//! ([`table_lens::put_segmented`]) against the held complements, takes the
//! source delta as the same differ over the source segments that changed,
//! and translates it forward, which must give back the edited view segment
//! for segment.
//!
//! The held manifests and runs change only once every run of a step has
//! sealed; a refusal or a stopped run leaves the lens and every segment as
//! they were, and retrying the same edit resumes its runs. A lens whose
//! restriction carries an expression is declined with
//! [`Residual::Global`](panproto_inst::segment::Residual::Global) when it is
//! bound, and a put whose source schema declares a key is declined when it
//! runs.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Debug;
use std::marker::PhantomData;

use panproto_inst::graph_edit::ComposeError;
use panproto_inst::identity::{ComplementShape, Digest};
use panproto_inst::segment::{
    AnySegment, Budget, CollectableStore, Collection, Partition, RunSpec, SegmentId,
    SegmentManifest, SegmentSource, SegmentStore, TableRestrict, apply_run, collect_garbage,
    run_segmented,
};
use panproto_inst::table_delta::TableDelta;
use panproto_inst::{FInstance, RestrictPlan};

use super::{
    Base, Complements, Family, FamilyReadError, Shared, apply_refusal, completed, differ_segments,
    emitted_segment, family_segment, put_refusal, restrict_refusal,
};
use crate::Lens;
use crate::graph_lens::HeldView;
use crate::shape_edit_lens::{EditShape, ShapeEditError, ShapeEditLens, get_refusal, incoherent};
use crate::table_lens;
use crate::table_shape::TableContext;

/// What a segmented table edit lens binds: the manifest sealing a source
/// family, its trusted digest, a source of its segments' canonical bytes,
/// and the store every run of the lens commits to.
pub struct SegmentedTable<Src, St> {
    /// The source manifest.
    pub manifest: SegmentManifest,
    /// Its trusted digest.
    pub input: Digest,
    /// The canonical bytes of the bound family's segments.
    pub segments: Src,
    /// The store the lens's runs commit to.
    pub store: St,
}

/// What every step of a segmented table edit lens runs under: the budget of
/// its runs and the lens's boundary protocols.
#[derive(Clone, Debug)]
pub struct SegmentedTableContext {
    /// The budget of every run.
    pub budget: Budget,
    /// The source and target protocols the table plan is compiled for.
    pub tables: TableContext,
}

/// The table shape held as segments: what [`ShapeEditLens`] needs to
/// translate table deltas through a lens without assembling the table.
pub struct SegmentedTableShape<Src, St>(PhantomData<fn() -> (Src, St)>);

/// A table lens bound to a table held as segments, translating table deltas
/// in both directions.
pub type SegmentedTableEditLens<Src, St> = ShapeEditLens<SegmentedTableShape<Src, St>>;

/// The state a [`SegmentedTableEditLens`] holds.
///
/// It holds the compiled table plan, the source of the bound family, the
/// store, the source and view manifests, where each source segment an edit
/// rewrote lives, and the run whose Emit artifacts hold the view and
/// complement segments.
pub struct SegmentedTableHeld<Src, St> {
    plan: RestrictPlan,
    lens: Digest,
    view_schema: Digest,
    segments: Src,
    store: RefCell<St>,
    source: SegmentManifest,
    source_digest: Digest,
    rewritten: BTreeMap<SegmentId, Digest>,
    view: SegmentManifest,
    get_run: Digest,
    reads: Vec<SegmentId>,
}

impl<Src, St> Debug for SegmentedTableHeld<Src, St> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SegmentedTableHeld")
            .field("source", &self.source_digest)
            .field("rewritten", &self.rewritten)
            .field("view", &self.view.digest())
            .field("get_run", &self.get_run)
            .field("reads", &self.reads)
            .finish_non_exhaustive()
    }
}

/// A translated step, ready to commit: the manifests and runs the lens
/// holds after it, its view delta, and the segments its runs read.
pub struct SegmentedTableStaged {
    source: SegmentManifest,
    source_digest: Digest,
    rewritten: BTreeMap<SegmentId, Digest>,
    view: SegmentManifest,
    get_run: Digest,
    view_delta: TableDelta,
    reads: Vec<SegmentId>,
}

/// The rows a table delta replaces, and the source rows of the pairs it
/// replaces.
fn table_replaced(input: &TableDelta) -> Vec<u32> {
    let rows = input
        .del_rows()
        .keys()
        .filter(|id| input.add_rows().contains_key(id))
        .map(|id| id.0);
    let pairs = input
        .del_pairs()
        .intersection(input.add_pairs())
        .map(|(_, src, _)| src.0);
    rows.chain(pairs).collect()
}

/// The table the segments `ids` hold, each read by `segment`: their owned
/// rows, and the pairs they hold whose rows are among those rows.
fn held_table(
    ids: &BTreeSet<SegmentId>,
    segment: &mut dyn FnMut(SegmentId) -> Result<AnySegment, ShapeEditError>,
) -> Result<FInstance, ShapeEditError> {
    let mut table = FInstance::new();
    let mut pairs = BTreeSet::new();
    for id in ids {
        let AnySegment::Table(held) = segment(*id)? else {
            return Err(incoherent("a segment of another shape"));
        };
        for (row, (name, cells)) in held.rows {
            table
                .insert_row(name, row, cells)
                .map_err(|error| incoherent(error.to_string()))?;
        }
        pairs.extend(held.pairs);
    }
    for (edge, src, tgt) in pairs {
        if table.row(src).is_some() && table.row(tgt).is_some() {
            table
                .insert_foreign_key(edge, src, tgt)
                .map_err(|error| incoherent(error.to_string()))?;
        }
    }
    Ok(table)
}

/// The incidence-closed replacement differ between the two families
/// `before` and `after` seal under `partition`, keeping the replacements of
/// `input`, computed over the segments [`differ_segments`] names.
fn family_delta(
    (before, after, partition): (&SegmentManifest, &SegmentManifest, &Partition),
    read_before: &mut dyn FnMut(SegmentId) -> Result<AnySegment, ShapeEditError>,
    read_after: &mut dyn FnMut(SegmentId) -> Result<AnySegment, ShapeEditError>,
    input: &TableDelta,
) -> Result<TableDelta, ShapeEditError> {
    let read = differ_segments((before, after), partition, table_replaced(input));
    let old = held_table(&read, read_before)?;
    let new = held_table(&read, read_after)?;
    Ok(TableDelta::diff_keeping_replacements(&old, &new, input))
}

impl<Src: SegmentSource, St: SegmentStore> SegmentedTableHeld<Src, St> {
    /// The source manifest the lens holds.
    #[must_use]
    pub const fn source_manifest(&self) -> &SegmentManifest {
        &self.source
    }

    /// The view manifest the lens holds.
    #[must_use]
    pub const fn view_manifest(&self) -> &SegmentManifest {
        &self.view
    }

    /// The restriction run whose Emit artifacts hold the view and
    /// complement segments.
    #[must_use]
    pub const fn get_run(&self) -> Digest {
        self.get_run
    }

    /// The store the lens's runs commit to.
    #[must_use]
    pub fn store(&self) -> std::cell::Ref<'_, St> {
        self.store.borrow()
    }

    /// The runs whose committed artifacts the held state reads: the
    /// restriction whose Emit artifacts hold the view and complement
    /// segments, and every edit run whose Emit artifacts hold a source
    /// segment an edit rewrote.
    #[must_use]
    pub fn live_runs(&self) -> BTreeSet<Digest> {
        std::iter::once(self.get_run)
            .chain(self.rewritten.values().copied())
            .collect()
    }

    /// Collect the store from [`Self::live_runs`] ([`collect_garbage`]).
    ///
    /// # Errors
    ///
    /// The store's first error; nothing the lens reads is removed.
    pub fn collect_garbage(&self) -> Result<Collection, St::Error>
    where
        St: CollectableStore,
    {
        collect_garbage(&mut *self.store.borrow_mut(), &self.live_runs())
    }

    /// The segments the runs of the last admitted step read from the
    /// family, in the order they read them: the edit run's read set and the
    /// segments the restriction did not carry over.
    #[must_use]
    pub fn reads(&self) -> &[SegmentId] {
        &self.reads
    }

    /// The canonical bytes of source segment `segment`.
    ///
    /// # Errors
    ///
    /// The failure of the source or the store.
    pub fn source_segment(&self, segment: SegmentId) -> Result<Vec<u8>, FamilyReadError> {
        let partition = self
            .source
            .partition()
            .map_err(|error| FamilyReadError(error.to_string()))?;
        Family::new(
            Base::Source(&self.segments),
            &self.store,
            &self.rewritten,
            (&partition, self.source.schema),
        )
        .read(segment)
    }

    /// Restrict the family `manifest` seals, read through `family`, carrying
    /// tasks over from the held get.
    fn restrict(
        &self,
        manifest: &SegmentManifest,
        family: &Family<'_, Src, St>,
        budget: Budget,
    ) -> Result<(SegmentManifest, Digest), ShapeEditError> {
        let run = run_segmented(
            &TableRestrict { plan: &self.plan },
            manifest,
            family,
            RunSpec {
                lens: self.lens,
                input: manifest.digest(),
                output_schema: self.view_schema,
                cancel_after_tasks: None,
                reuse: Some(self.source_digest),
            },
            budget,
            &mut Shared(&self.store),
            &mut |_| {},
        );
        completed(run, restrict_refusal)
    }

    /// Apply `delta` to the source family and restrict the edited family.
    fn forward(
        &self,
        budget: Budget,
        delta: &TableDelta,
    ) -> Result<SegmentedTableStaged, ShapeEditError> {
        let partition = self.source.partition().map_err(ShapeEditError::Segment)?;
        let at = (&partition, self.source.schema);
        let family = Family::new(
            Base::Source(&self.segments),
            &self.store,
            &self.rewritten,
            at,
        );
        let applied = apply_run(
            delta,
            &self.source,
            &family,
            RunSpec {
                lens: self.lens,
                input: self.source_digest,
                output_schema: self.source.schema,
                cancel_after_tasks: None,
                reuse: None,
            },
            budget,
            &mut Shared(&self.store),
            &mut |_| {},
        );
        let (source, apply) = completed(applied, apply_refusal)?;
        let mut reads = family.reads.into_inner();
        let mut rewritten = self.rewritten.clone();
        for record in source.segments.iter().filter(|record| {
            self.source.record(record.id).map(|r| r.content) != Some(record.content)
        }) {
            rewritten.insert(record.id, apply);
        }
        let edited = Family::new(Base::Source(&self.segments), &self.store, &rewritten, at);
        let (view, get_run) = self.restrict(&source, &edited, budget)?;
        reads.extend(edited.reads.into_inner());
        let view_partition = view.partition().map_err(ShapeEditError::Segment)?;
        let view_delta = family_delta(
            (&self.view, &view, &view_partition),
            &mut |id| emitted_segment(&self.store, self.get_run, &view_partition, id),
            &mut |id| emitted_segment(&self.store, get_run, &view_partition, id),
            delta,
        )?;
        Ok(SegmentedTableStaged {
            source_digest: source.digest(),
            source,
            rewritten,
            view,
            get_run,
            view_delta,
            reads,
        })
    }

    /// Apply the view delta `edit` to the view family and put the edited
    /// view back: the edited view manifest, and the source manifest and run
    /// of the put.
    fn put_back(
        &self,
        lens: &Lens,
        context: &SegmentedTableContext,
        edit: &TableDelta,
    ) -> Result<(SegmentManifest, SegmentManifest, Digest), ShapeEditError> {
        let budget = context.budget;
        let partition = self.view.partition().map_err(ShapeEditError::Segment)?;
        let at = (&partition, self.view.schema);
        let unchanged = BTreeMap::new();
        let views = Family::new(Base::<Src>::Run(self.get_run), &self.store, &unchanged, at);
        let applied = apply_run(
            edit,
            &self.view,
            &views,
            RunSpec {
                lens: self.lens,
                input: self.view.digest(),
                output_schema: self.view.schema,
                cancel_after_tasks: None,
                reuse: None,
            },
            budget,
            &mut Shared(&self.store),
            &mut |_| {},
        );
        let (edited, apply) = completed(applied, apply_refusal)?;
        let rewritten: BTreeMap<SegmentId, Digest> = edited
            .segments
            .iter()
            .filter(|record| self.view.record(record.id).map(|r| r.content) != Some(record.content))
            .map(|record| (record.id, apply))
            .collect();
        let views = Family::new(Base::<Src>::Run(self.get_run), &self.store, &rewritten, at);
        let complements = Complements {
            store: &self.store,
            run: self.get_run,
            partition: &partition,
        };
        let put = table_lens::put_segmented(
            lens,
            context.tables.protocols(),
            &HeldView {
                manifest: &edited,
                input: edited.digest(),
                views: &views,
                complements: &complements,
            },
            budget,
            &mut Shared(&self.store),
            None,
        )
        .map_err(|error| ShapeEditError::Put(Box::new(error)))?;
        let (restored, put_run) = completed(put, put_refusal)?;
        Ok((edited, restored, put_run))
    }
}

impl<Src: SegmentSource, St: SegmentStore> EditShape for SegmentedTableShape<Src, St> {
    type Source = SegmentedTable<Src, St>;
    type Delta = TableDelta;
    type Context = SegmentedTableContext;
    type Held = SegmentedTableHeld<Src, St>;
    type Staged = SegmentedTableStaged;

    fn bind(
        lens: &Lens,
        context: &SegmentedTableContext,
        source: SegmentedTable<Src, St>,
    ) -> Result<SegmentedTableHeld<Src, St>, ShapeEditError> {
        let plan = RestrictPlan::compile_for_tables(
            &lens.src_schema,
            &lens.tgt_schema,
            &lens.compiled,
            context.tables.protocols(),
        )
        .map_err(get_refusal)?;
        let identity = lens.identity(ComplementShape::Functor);
        let SegmentedTable {
            manifest,
            input,
            segments,
            store,
        } = source;
        let store = RefCell::new(store);
        let partition = manifest.partition().map_err(ShapeEditError::Segment)?;
        let unchanged = BTreeMap::new();
        let family = Family::new(
            Base::Source(&segments),
            &store,
            &unchanged,
            (&partition, manifest.schema),
        );
        let run = run_segmented(
            &TableRestrict { plan: &plan },
            &manifest,
            &family,
            RunSpec {
                lens: identity.lens,
                input,
                output_schema: identity.target_schema,
                cancel_after_tasks: None,
                reuse: None,
            },
            context.budget,
            &mut Shared(&store),
            &mut |_| {},
        );
        let (view, get_run) = completed(run, restrict_refusal)?;
        let reads = family.reads.into_inner();
        Ok(SegmentedTableHeld {
            plan,
            lens: identity.lens,
            view_schema: identity.target_schema,
            segments,
            store,
            source: manifest,
            source_digest: input,
            rewritten: BTreeMap::new(),
            view,
            get_run,
            reads,
        })
    }

    fn stage(
        _lens: &Lens,
        context: &SegmentedTableContext,
        held: &mut SegmentedTableHeld<Src, St>,
        delta: &TableDelta,
    ) -> Result<SegmentedTableStaged, ShapeEditError> {
        held.forward(context.budget, delta)
    }

    fn stage_put(
        lens: &Lens,
        context: &SegmentedTableContext,
        held: &mut SegmentedTableHeld<Src, St>,
        edit: &TableDelta,
    ) -> Result<(SegmentedTableStaged, TableDelta), ShapeEditError> {
        let (edited, restored, put_run) = held.put_back(lens, context, edit)?;
        let partition = held.source.partition().map_err(ShapeEditError::Segment)?;
        let family = Family::new(
            Base::Source(&held.segments),
            &held.store,
            &held.rewritten,
            (&partition, held.source.schema),
        );
        let source_delta = family_delta(
            (&held.source, &restored, &partition),
            &mut |id| family_segment(&family, id),
            &mut |id| emitted_segment(&held.store, put_run, &partition, id),
            edit,
        )?;
        let mut staged = held.forward(context.budget, &source_delta)?;
        let agrees = edited.segments.len() == staged.view.segments.len()
            && edited
                .segments
                .iter()
                .zip(&staged.view.segments)
                .all(|(edited, forward)| {
                    edited.id == forward.id && edited.content == forward.content
                });
        if !agrees {
            return Err(ShapeEditError::PutGetMismatch);
        }
        let mut reads = family.reads.into_inner();
        reads.append(&mut staged.reads);
        staged.reads = reads;
        staged.view_delta = edit.clone();
        Ok((staged, source_delta))
    }

    fn staged_view_delta(staged: &SegmentedTableStaged) -> &TableDelta {
        &staged.view_delta
    }

    fn abort(
        _held: &mut SegmentedTableHeld<Src, St>,
        _staged: &SegmentedTableStaged,
    ) -> Result<(), ShapeEditError> {
        Ok(())
    }

    fn commit(
        held: &mut SegmentedTableHeld<Src, St>,
        staged: SegmentedTableStaged,
    ) -> Result<(), ShapeEditError> {
        let SegmentedTableStaged {
            source,
            source_digest,
            rewritten,
            view,
            get_run,
            reads,
            ..
        } = staged;
        held.source = source;
        held.source_digest = source_digest;
        held.rewritten = rewritten;
        held.view = view;
        held.get_run = get_run;
        held.reads = reads;
        Ok(())
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
}

impl<Src: SegmentSource, St: SegmentStore> ShapeEditLens<SegmentedTableShape<Src, St>> {
    /// Bind `lens` to the source family `table`: run segmented table get
    /// over it under `context` and hold the source and view manifests.
    ///
    /// # Errors
    ///
    /// [`ShapeEditError::Get`] when the table plan does not compile or get
    /// refuses the family, [`ShapeEditError::Residual`] when the run is
    /// declined (a restriction with an expression is
    /// [`Residual::Global`](panproto_inst::segment::Residual::Global)),
    /// [`ShapeEditError::Segment`] for a family the run refuses, and
    /// [`ShapeEditError::Incomplete`] when the store or the source fails.
    pub fn segmented_table(
        lens: Lens,
        context: SegmentedTableContext,
        table: SegmentedTable<Src, St>,
    ) -> Result<Self, ShapeEditError> {
        Self::bind(lens, context, table)
    }

    /// The held state: the source and view manifests, the store and the
    /// segments the last step read.
    #[must_use]
    pub const fn segmented_table_state(&self) -> &SegmentedTableHeld<Src, St> {
        self.held()
    }
}
