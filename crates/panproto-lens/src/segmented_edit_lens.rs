//! Graph edit lenses over a graph held as segments: the edit lens of a
//! graph lens whose source, view and complement live in a segment store.
//!
//! A [`SegmentedGraphEditLens`] is the [`ShapeEditLens`] of the segmented
//! graph shape ([`SegmentedGraphShape`]). It binds a sealed source family
//! ([`SegmentedGraph`]) by running segmented get over it, and holds the
//! source manifest, the view manifest of that get and the run whose Emit
//! artifacts hold the view and complement segments. No step assembles the
//! source or the view.
//!
//! [`ShapeEditLens::get_edit`] translates a source [`GraphDelta`] in three
//! runs against the store:
//!
//! 1. the delta is applied to the source family as a checkpointed edit
//!    ([`apply_run`]), which reads and rewrites only the segments the delta
//!    can change;
//! 2. the edited family is restricted as a checkpointed run of
//!    [`GraphRestrict`] holding the complements of the previous get, so
//!    every synthesized element whose synthesis key persists keeps its
//!    identifier, and carrying tasks over from that get
//!    ([`RunSpec::reuse`]), so only the segments whose records changed are
//!    read again;
//! 3. the view delta is the incidence-closed replacement differ between the
//!    held view and the new view, keeping the replacements of the source
//!    delta, computed over the view segments whose records changed and the
//!    segments their boundaries name.
//!
//! [`ShapeEditLens::put_edit`] applies a view delta to the view family,
//! puts the edited view back segment by segment ([`put_segmented_run`])
//! against the held complements, takes the source delta as the same differ
//! over the source segments that changed, and translates it forward, which
//! must give back the edited view segment for segment.
//!
//! The held manifests and runs change only once every run of a step has
//! sealed; a refusal or a stopped run leaves the lens and every segment
//! as they were, and the artifacts the step committed are left in the store.
//! Retrying the same edit resumes its runs. A lens whose restriction carries
//! an expression is declined with
//! [`Residual::Global`](panproto_inst::segment::Residual::Global) when it is
//! bound.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Debug;
use std::marker::PhantomData;

use panproto_inst::graph_edit::ComposeError;
use panproto_inst::identity::{ComplementShape, Digest};
use panproto_inst::segment::{
    AnySegment, ArtifactKey, Budget, GraphRestrict, Partition, RunIncompleteReason, RunSpec,
    RunStatus, SegmentError, SegmentId, SegmentManifest, SegmentSource, SegmentStore,
    SegmentedApplyError, SegmentedRestrictError, SegmentedRun, TaskRecord, apply_run,
    read_emitted_segment, run_segmented,
};
use panproto_inst::{GInstance, GraphDelta, RestrictPlan};

use crate::Lens;
use crate::graph_lens::{HeldView, SegmentedPutError, put_segmented_run};
use crate::shape_edit_lens::{EditShape, ShapeEditError, ShapeEditLens, get_refusal, incoherent};

/// What a segmented graph edit lens binds: the manifest sealing a source
/// family, its trusted digest, a source of its segments' canonical bytes,
/// and the store every run of the lens commits to.
pub struct SegmentedGraph<Src, St> {
    /// The source manifest.
    pub manifest: SegmentManifest,
    /// Its trusted digest.
    pub input: Digest,
    /// The canonical bytes of the bound family's segments.
    pub segments: Src,
    /// The store the lens's runs commit to.
    pub store: St,
}

/// The graph shape held as segments: what [`ShapeEditLens`] needs to
/// translate graph deltas through a lens without assembling the graph.
pub struct SegmentedGraphShape<Src, St>(PhantomData<fn() -> (Src, St)>);

/// A graph lens bound to a graph held as segments, translating graph deltas
/// in both directions.
pub type SegmentedGraphEditLens<Src, St> = ShapeEditLens<SegmentedGraphShape<Src, St>>;

/// The state a [`SegmentedGraphEditLens`] holds.
///
/// It holds the compiled plan, the source of the bound family, the store,
/// the source and view manifests, where each source segment an edit rewrote
/// lives, and the run whose Emit artifacts hold the view and complement
/// segments.
pub struct SegmentedGraphHeld<Src, St> {
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

impl<Src: Clone, St: Clone> Clone for SegmentedGraphHeld<Src, St> {
    fn clone(&self) -> Self {
        Self {
            plan: self.plan.clone(),
            lens: self.lens,
            view_schema: self.view_schema,
            segments: self.segments.clone(),
            store: self.store.clone(),
            source: self.source.clone(),
            source_digest: self.source_digest,
            rewritten: self.rewritten.clone(),
            view: self.view.clone(),
            get_run: self.get_run,
            reads: self.reads.clone(),
        }
    }
}

impl<Src, St> Debug for SegmentedGraphHeld<Src, St> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SegmentedGraphHeld")
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
pub struct SegmentedGraphStaged {
    source: SegmentManifest,
    source_digest: Digest,
    rewritten: BTreeMap<SegmentId, Digest>,
    view: SegmentManifest,
    get_run: Digest,
    view_delta: GraphDelta,
    reads: Vec<SegmentId>,
}

/// Why a segment could not be read from a family.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct FamilyReadError(String);

/// Where a family's segments live when no edit rewrote them.
enum Base<'a, Src> {
    /// The caller's source of the bound family.
    Source(&'a Src),
    /// The Emit artifacts of a run in the store.
    Run(Digest),
}

/// A family of segments read one at a time: a segment an edit rewrote from
/// the Emit artifact of that edit's run, every other from the base. The
/// segments read are recorded in order.
struct Family<'a, Src, St> {
    base: Base<'a, Src>,
    store: &'a RefCell<St>,
    rewritten: &'a BTreeMap<SegmentId, Digest>,
    partition: &'a Partition,
    schema: Digest,
    reads: RefCell<Vec<SegmentId>>,
}

impl<'a, Src, St> Family<'a, Src, St> {
    const fn new(
        base: Base<'a, Src>,
        store: &'a RefCell<St>,
        rewritten: &'a BTreeMap<SegmentId, Digest>,
        (partition, schema): (&'a Partition, Digest),
    ) -> Self {
        Self {
            base,
            store,
            rewritten,
            partition,
            schema,
            reads: RefCell::new(Vec::new()),
        }
    }
}

/// The canonical bytes, under `schema`, of the segment run `run` emitted
/// for `segment`.
fn emitted_bytes<St: SegmentStore>(
    store: &RefCell<St>,
    run: Digest,
    (partition, schema): (&Partition, Digest),
    segment: SegmentId,
) -> Result<Vec<u8>, SegmentError> {
    let (emitted, _) = read_emitted_segment(&*store.borrow(), run, partition, segment)?;
    emitted.canonical_bytes(&schema, partition)
}

impl<Src: SegmentSource, St: SegmentStore> SegmentSource for Family<'_, Src, St> {
    type Error = FamilyReadError;

    fn read(&self, segment: SegmentId) -> Result<Vec<u8>, FamilyReadError> {
        self.reads.borrow_mut().push(segment);
        let at = (self.partition, self.schema);
        match (self.rewritten.get(&segment), &self.base) {
            (Some(run), _) | (None, Base::Run(run)) => emitted_bytes(self.store, *run, at, segment)
                .map_err(|error| FamilyReadError(error.to_string())),
            (None, Base::Source(source)) => source
                .read(segment)
                .map_err(|error| FamilyReadError(error.to_string())),
        }
    }
}

/// The complement segments a restriction run emitted, read one at a time.
struct Complements<'a, St> {
    store: &'a RefCell<St>,
    run: Digest,
    partition: &'a Partition,
}

impl<St: SegmentStore> SegmentSource for Complements<'_, St> {
    type Error = FamilyReadError;

    fn read(&self, segment: SegmentId) -> Result<Vec<u8>, FamilyReadError> {
        read_emitted_segment(&*self.store.borrow(), self.run, self.partition, segment)
            .map(|(_, complement)| complement)
            .map_err(|error| FamilyReadError(error.to_string()))
    }
}

/// A store shared between a run and the families it reads, each call
/// borrowing it for the call alone.
struct Shared<'a, St>(&'a RefCell<St>);

impl<St: SegmentStore> SegmentStore for Shared<'_, St> {
    type Error = St::Error;

    fn read(&self, key: &ArtifactKey) -> Result<Option<Vec<u8>>, St::Error> {
        self.0.borrow().read(key)
    }

    fn read_at(
        &self,
        key: &ArtifactKey,
        offset: u64,
        len: u64,
    ) -> Result<Option<Vec<u8>>, St::Error> {
        self.0.borrow().read_at(key, offset, len)
    }

    fn stage(&mut self, key: &ArtifactKey, bytes: Vec<u8>) -> Result<(), St::Error> {
        self.0.borrow_mut().stage(key, bytes)
    }

    fn commit(&mut self, key: &ArtifactKey, record: &TaskRecord) -> Result<(), St::Error> {
        self.0.borrow_mut().commit(key, record)
    }

    fn journal(&self, run: &Digest) -> Result<panproto_inst::segment::RunJournal, St::Error> {
        self.0.borrow().journal(run)
    }

    fn staged_uncommitted(&self, run: &Digest) -> Result<Vec<ArtifactKey>, St::Error> {
        self.0.borrow().staged_uncommitted(run)
    }
}

/// The manifest and identity of a complete run, or the error its status
/// gives, a refusal mapped by `refused`.
fn completed<E>(
    run: SegmentedRun<E>,
    refused: impl FnOnce(E) -> ShapeEditError,
) -> Result<(SegmentManifest, Digest), ShapeEditError> {
    match (run.report.status, run.manifest, run.run) {
        (RunStatus::Complete { .. }, Some(manifest), Some(id)) => Ok((manifest, id)),
        (RunStatus::Complete { .. }, ..) => {
            Err(incoherent("a complete run has no manifest or identity"))
        }
        (RunStatus::Failed(error), ..) => Err(refused(error)),
        (RunStatus::Residual(residual), ..) => Err(ShapeEditError::Residual(residual)),
        (RunStatus::Incomplete { reason }, ..) => Err(ShapeEditError::Incomplete(reason)),
    }
}

/// The refusal of a restriction run, as the edit lens reports it.
fn restrict_refusal(error: SegmentedRestrictError) -> ShapeEditError {
    match error {
        SegmentedRestrictError::Restrict { error, .. } => get_refusal(error),
        SegmentedRestrictError::Segment(error) => ShapeEditError::Segment(error),
        SegmentedRestrictError::Residual(residual) => ShapeEditError::Residual(residual),
        other => incoherent(other.to_string()),
    }
}

/// The refusal of an edit run, as the edit lens reports it.
fn apply_refusal(error: SegmentedApplyError) -> ShapeEditError {
    match error {
        SegmentedApplyError::Edit(error) => ShapeEditError::Apply(error),
        SegmentedApplyError::Segment(error) => ShapeEditError::Segment(error),
        other => incoherent(other.to_string()),
    }
}

/// The refusal of a put run, as the edit lens reports it.
fn put_refusal(error: SegmentedPutError) -> ShapeEditError {
    match error {
        SegmentedPutError::Lens(error) => ShapeEditError::Put(Box::new(error)),
        SegmentedPutError::Segment(error) => ShapeEditError::Segment(error),
        SegmentedPutError::Residual(residual) => ShapeEditError::Residual(residual),
    }
}

/// The segments the differ of `input` from the family `before` seals to
/// the one `after` seals must read: every segment whose record differs by
/// content, every segment owning an element `input` replaces (whose
/// replacement the differ keeps even when the payloads agree), and every
/// segment either record of those names as a cross peer, which holds the
/// edges entering them.
fn differ_segments(
    (before, after): (&SegmentManifest, &SegmentManifest),
    partition: &Partition,
    input: &GraphDelta,
) -> BTreeSet<SegmentId> {
    let content = |manifest: &SegmentManifest, id: SegmentId| {
        manifest.record(id).map(|record| record.content)
    };
    let mut touched: BTreeSet<SegmentId> = after
        .segments
        .iter()
        .map(|record| record.id)
        .chain(before.segments.iter().map(|record| record.id))
        .filter(|id| content(before, *id) != content(after, *id))
        .collect();
    let replaced_nodes = input
        .del_nodes()
        .keys()
        .filter(|id| input.add_nodes().contains_key(id))
        .copied();
    let replaced_edges = input
        .del_edges()
        .iter()
        .filter_map(|(id, old)| input.add_edges().get(id).map(|new| [old.src, new.src]))
        .flatten();
    touched.extend(
        replaced_nodes
            .chain(replaced_edges)
            .filter_map(|node| partition.owner_of_node(node)),
    );
    let mut read = touched.clone();
    for id in &touched {
        for manifest in [before, after] {
            if let Some(record) = manifest.record(*id) {
                read.extend(record.cross_out.keys().copied());
                read.extend(record.cross_in.keys().copied());
            }
        }
    }
    read
}

/// The graph the segments `ids` hold, each read by `segment`: their owned
/// nodes, and the edges they hold whose endpoints are among those nodes.
fn held_graph(
    ids: &BTreeSet<SegmentId>,
    segment: &mut dyn FnMut(SegmentId) -> Result<AnySegment, ShapeEditError>,
) -> Result<GInstance, ShapeEditError> {
    let mut graph = GInstance::new();
    let mut edges = BTreeMap::new();
    for id in ids {
        let AnySegment::Graph(held) = segment(*id)? else {
            return Err(incoherent("a segment of another shape"));
        };
        for node in held.nodes.into_values() {
            graph = graph.with_node(node);
        }
        edges.extend(held.edges);
    }
    for (id, edge) in edges {
        if graph.node(edge.src).is_some() && graph.node(edge.tgt).is_some() {
            graph
                .insert_edge(id, edge)
                .map_err(|error| incoherent(error.to_string()))?;
        }
    }
    Ok(graph)
}

/// The incidence-closed replacement differ between the two families
/// `before` and `after` seal under `partition`, keeping the replacements of
/// `input`, computed over the segments [`differ_segments`] names; each
/// segment is read through `read_before` or `read_after`.
fn family_delta(
    (before, after, partition): (&SegmentManifest, &SegmentManifest, &Partition),
    read_before: &mut dyn FnMut(SegmentId) -> Result<AnySegment, ShapeEditError>,
    read_after: &mut dyn FnMut(SegmentId) -> Result<AnySegment, ShapeEditError>,
    input: &GraphDelta,
) -> Result<GraphDelta, ShapeEditError> {
    let read = differ_segments((before, after), partition, input);
    let old = held_graph(&read, read_before)?;
    let new = held_graph(&read, read_after)?;
    GraphDelta::diff_keeping_replacements(&old, &new, input)
        .map_err(|error| incoherent(error.to_string()))
}

/// The segment `segment` of `family`, decoded.
fn family_segment<Src: SegmentSource, St: SegmentStore>(
    family: &Family<'_, Src, St>,
    segment: SegmentId,
) -> Result<AnySegment, ShapeEditError> {
    let bytes = family
        .read(segment)
        .map_err(|error| ShapeEditError::Incomplete(store_reason(&error)))?;
    Ok(AnySegment::from_canonical_bytes(&bytes, family.partition)
        .map_err(ShapeEditError::Segment)?
        .0)
}

/// The segment `run` emitted for `segment`, decoded.
fn emitted_segment<St: SegmentStore>(
    store: &RefCell<St>,
    run: Digest,
    partition: &Partition,
    segment: SegmentId,
) -> Result<AnySegment, ShapeEditError> {
    read_emitted_segment(&*store.borrow(), run, partition, segment)
        .map(|(emitted, _)| emitted)
        .map_err(ShapeEditError::Segment)
}

/// A read failure as the reason a step stopped.
fn store_reason(error: &impl std::fmt::Display) -> RunIncompleteReason {
    RunIncompleteReason::Store {
        message: error.to_string(),
    }
}

impl<Src: SegmentSource, St: SegmentStore> SegmentedGraphHeld<Src, St> {
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

    /// The segments the runs of the last admitted step read from the
    /// family, in the order they read them: the edit run's read set and
    /// the segments the restriction did not carry over.
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

    /// Restrict the family `manifest` seals, read through `family`, holding
    /// the complements of the held get and carrying tasks over from it.
    fn restrict(
        &self,
        manifest: &SegmentManifest,
        family: &Family<'_, Src, St>,
        budget: Budget,
    ) -> Result<(SegmentManifest, Digest), ShapeEditError> {
        let complements = Complements {
            store: &self.store,
            run: self.get_run,
            partition: family.partition,
        };
        let run = run_segmented(
            &GraphRestrict {
                plan: &self.plan,
                held: Some(&complements),
            },
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
        delta: &GraphDelta,
    ) -> Result<SegmentedGraphStaged, ShapeEditError> {
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
        for (id, _) in source
            .segments
            .iter()
            .map(|record| (record.id, record.content))
            .filter(|(id, content)| self.source.record(*id).map(|r| r.content) != Some(*content))
        {
            rewritten.insert(id, apply);
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
        Ok(SegmentedGraphStaged {
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
    /// view back: the source manifest and run of the put.
    fn put_back(
        &self,
        lens: &Lens,
        budget: Budget,
        edit: &GraphDelta,
    ) -> Result<(SegmentManifest, SegmentManifest, Digest), ShapeEditError> {
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
        let put = put_segmented_run(
            lens,
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
        .map_err(put_refusal)?;
        let (restored, put_run) = completed(put, put_refusal)?;
        Ok((edited, restored, put_run))
    }
}

impl<Src: SegmentSource, St: SegmentStore> EditShape for SegmentedGraphShape<Src, St> {
    type Source = SegmentedGraph<Src, St>;
    type Delta = GraphDelta;
    type Context = Budget;
    type Held = SegmentedGraphHeld<Src, St>;
    type Staged = SegmentedGraphStaged;

    fn bind(
        lens: &Lens,
        budget: &Budget,
        source: SegmentedGraph<Src, St>,
    ) -> Result<SegmentedGraphHeld<Src, St>, ShapeEditError> {
        let plan = RestrictPlan::compile(&lens.src_schema, &lens.tgt_schema, &lens.compiled)
            .map_err(get_refusal)?;
        let identity = lens.identity(ComplementShape::Graph);
        let SegmentedGraph {
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
            &GraphRestrict {
                plan: &plan,
                held: None,
            },
            &manifest,
            &family,
            RunSpec {
                lens: identity.lens,
                input,
                output_schema: identity.target_schema,
                cancel_after_tasks: None,
                reuse: None,
            },
            *budget,
            &mut Shared(&store),
            &mut |_| {},
        );
        let (view, get_run) = completed(run, restrict_refusal)?;
        let reads = family.reads.into_inner();
        Ok(SegmentedGraphHeld {
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
        budget: &Budget,
        held: &mut SegmentedGraphHeld<Src, St>,
        delta: &GraphDelta,
    ) -> Result<SegmentedGraphStaged, ShapeEditError> {
        held.forward(*budget, delta)
    }

    fn stage_put(
        lens: &Lens,
        budget: &Budget,
        held: &mut SegmentedGraphHeld<Src, St>,
        edit: &GraphDelta,
    ) -> Result<(SegmentedGraphStaged, GraphDelta), ShapeEditError> {
        let (edited, restored, put_run) = held.put_back(lens, *budget, edit)?;
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
        let mut staged = held.forward(*budget, &source_delta)?;
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

    fn staged_view_delta(staged: &SegmentedGraphStaged) -> &GraphDelta {
        &staged.view_delta
    }

    fn abort(
        _held: &mut SegmentedGraphHeld<Src, St>,
        _staged: &SegmentedGraphStaged,
    ) -> Result<(), ShapeEditError> {
        Ok(())
    }

    fn commit(
        held: &mut SegmentedGraphHeld<Src, St>,
        staged: SegmentedGraphStaged,
    ) -> Result<(), ShapeEditError> {
        let SegmentedGraphStaged {
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

    fn identity() -> GraphDelta {
        GraphDelta::identity()
    }

    fn compose(first: &GraphDelta, next: &GraphDelta) -> Result<GraphDelta, ComposeError> {
        first.compose(next)
    }

    fn digest(delta: &GraphDelta) -> Digest {
        delta.digest()
    }
}

impl<Src: SegmentSource, St: SegmentStore> ShapeEditLens<SegmentedGraphShape<Src, St>> {
    /// Bind `lens` to the source family `graph`: run segmented get over it
    /// under `budget` and hold the source and view manifests.
    ///
    /// # Errors
    ///
    /// [`ShapeEditError::Get`] when the plan does not compile or get refuses
    /// the family, [`ShapeEditError::Residual`] when the run is declined (a
    /// restriction with an expression is
    /// [`Residual::Global`](panproto_inst::segment::Residual::Global)),
    /// [`ShapeEditError::Segment`] for a family the run refuses, and
    /// [`ShapeEditError::Incomplete`] when the store or the source fails.
    pub fn segmented(
        lens: Lens,
        budget: Budget,
        graph: SegmentedGraph<Src, St>,
    ) -> Result<Self, ShapeEditError> {
        Self::bind(lens, budget, graph)
    }

    /// The held state: the source and view manifests, the store and the
    /// segments the last step read.
    #[must_use]
    pub const fn segmented_state(&self) -> &SegmentedGraphHeld<Src, St> {
        self.held()
    }
}
