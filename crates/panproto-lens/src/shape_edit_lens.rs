//! Edit lenses over every instance shape that has a delta algebra: a lens
//! bound to a source that translates deltas in both directions and keeps
//! its view and complement current, edit by edit.
//!
//! A [`ShapeEditLens`] holds a lens, the shape's context, and the shape's
//! held state: a source, the view and complement get gives for it, and
//! whatever indexes the shape keeps. [`ShapeEditLens::get_edit`] takes a
//! source delta, applies it, and returns the view delta that takes the held
//! view to the view of the edited source; [`ShapeEditLens::put_edit`] takes
//! a view delta, puts the edited view, and returns the source delta. Like
//! the edit lenses of `hofmann2012edit`, it translates edits rather than
//! states and keeps a complement that each translated edit updates; here
//! the held source, view and complement are that complement, and edits are
//! partial, so a translation may refuse. Both directions are atomic: a
//! refusal leaves the lens as it was.
//!
//! A shape ([`EditShape`]) supplies its delta algebra and its two staged
//! steps. For a shape held in memory ([`WholeStateShape`]) the translations
//! are the same: a forward translation is the incidence-closed replacement
//! differ between the held view and the view of the edited source, keeping
//! the replacements of the input
//! ([`GraphDelta::diff_keeping_replacements`],
//! [`TableDelta::diff_keeping_replacements`]); a backward translation puts
//! the edited view, takes the same differ from the held source to the put
//! source, translates that delta forward, and requires the result to be the
//! edited view. A whole-state shape supplies its whole-state put and its
//! forward step: [`GraphEditLens`] runs the graph
//! restrict engine on the region an edit reaches and keeps synthesized ids
//! stable, and [`TableEditLens`] runs table get on the edited source, whose
//! rows and pairs carry no synthesized id.
//!
//! [`GraphDelta::diff_keeping_replacements`]: panproto_inst::GraphDelta::diff_keeping_replacements
//! [`TableDelta::diff_keeping_replacements`]: panproto_inst::table_delta::TableDelta::diff_keeping_replacements
//! [`GraphEditLens`]: crate::GraphEditLens
//! [`TableEditLens`]: crate::TableEditLens

use std::fmt::Debug;

use panproto_inst::EditError;
use panproto_inst::graph_edit::ComposeError;
use panproto_inst::identity::Digest;
use panproto_inst::segment::{IdKind, Residual, RunIncompleteReason, SegmentError};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::{Lens, LensError};

/// Why an edit lens refuses an edit. Every refusal leaves the lens
/// unchanged.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ShapeEditError {
    /// The edit does not apply to the held source (for `get_edit`) or the
    /// held view (for `put_edit`): a stale payload, a missing element or
    /// endpoint, a dangling edge or pair, a reused id or a malformed fan.
    #[error("the edit does not apply: {0}")]
    Apply(#[source] EditError),
    /// Get refuses the edited source, with the refusal a whole-state get of
    /// that source returns.
    #[error("get refuses the edited source: {0}")]
    Get(#[source] Box<LensError>),
    /// Put refuses the edited view, with the refusal a whole-state put
    /// returns: an envelope refusal or the first put-domain violation.
    #[error("put refuses the edited view: {0}")]
    Put(#[source] Box<LensError>),
    /// The edits of a batch do not compose.
    #[error("the batch does not compose: {0}")]
    Compose(#[source] ComposeError),
    /// No id below `u32::MAX` remains for a synthesized element.
    #[error("no {kind:?} id remains for a synthesized element")]
    IdsExhausted {
        /// Whether node or edge ids ran out.
        kind: IdKind,
    },
    /// The get of the put source does not give back the edited view.
    #[error("the source put returns does not get back the edited view")]
    PutGetMismatch,
    /// A segmented run refuses the family: a segment, manifest or artifact
    /// is malformed, or an edited family cannot be laid out under its
    /// partition.
    #[error("segment refusal: {0}")]
    Segment(#[source] SegmentError),
    /// A segmented run is declined within its budget or locality; a lens
    /// whose restriction carries an expression is
    /// [`Residual::Global`](panproto_inst::segment::Residual::Global).
    #[error("declined: {0}")]
    Residual(Residual),
    /// A segmented run stopped before it sealed, by a store or source
    /// failure; translating the same edit again resumes it.
    #[error("a run stopped before it sealed: {0:?}")]
    Incomplete(RunIncompleteReason),
    /// The held state broke an invariant of the construction; this names a
    /// defect, never a property of the edit.
    #[error("edit lens invariant broken: {0}")]
    Incoherent(String),
}

pub(crate) fn incoherent(detail: impl Into<String>) -> ShapeEditError {
    ShapeEditError::Incoherent(detail.into())
}

pub(crate) fn get_refusal(error: impl Into<LensError>) -> ShapeEditError {
    ShapeEditError::Get(Box::new(error.into()))
}

/// Which direction an edit was translated in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditDirection {
    /// A source edit translated to the view.
    Get,
    /// A view edit translated to the source.
    Put,
}

/// One admitted edit with its translation: the record an audit log keeps.
///
/// `source_delta` took the held source to the new one and `view_delta` the
/// held view to the new one; in the `Get` direction the first is the input
/// and in the `Put` direction the second is. `input` is the digest of the
/// input delta, the edit's identity, and `sequence` counts the edits the
/// lens has admitted, from one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound(serialize = "D: Serialize", deserialize = "D: DeserializeOwned"))]
pub struct ShapeEditStep<D> {
    /// The position of this edit among those the lens admitted, from one.
    pub sequence: u64,
    /// The direction the edit was translated in.
    pub direction: EditDirection,
    /// The digest of the input delta.
    pub input: Digest,
    /// The source delta.
    pub source_delta: D,
    /// The view delta.
    pub view_delta: D,
}

/// What [`ShapeEditLens`] needs of a shape: a delta algebra and the two
/// staged steps, forward and backward, each committed or aborted.
///
/// The held state may live anywhere: in memory ([`WholeStateShape`]), or in
/// a segment store, as the segmented graph shape holds it.
pub trait EditShape: Sized {
    /// What binding reads: the source instance, or the manifest and segments
    /// of a source held as segments.
    type Source;
    /// The deltas of the shape.
    type Delta: Clone + Debug + Eq + Serialize + DeserializeOwned;
    /// What get and put read besides the lens: the boundary protocols of a
    /// table, nothing for a graph, the run budget of a segmented graph.
    type Context: Clone + Debug;
    /// The held source, view and complement, with the shape's indexes.
    type Held;
    /// A step whose translation is computed but not committed.
    type Staged;

    /// Run get on `source` and hold it with its view and complement.
    ///
    /// # Errors
    ///
    /// [`ShapeEditError::Get`] with the refusal of get on `source`.
    fn bind(
        lens: &Lens,
        context: &Self::Context,
        source: Self::Source,
    ) -> Result<Self::Held, ShapeEditError>;

    /// Translate the source delta `delta` forward, leaving the held view
    /// and complement as they are; on a refusal the held state is as it
    /// was.
    ///
    /// # Errors
    ///
    /// The refusal of applying `delta` or of get on the edited source.
    fn stage(
        lens: &Lens,
        context: &Self::Context,
        held: &mut Self::Held,
        delta: &Self::Delta,
    ) -> Result<Self::Staged, ShapeEditError>;

    /// Translate the view delta `delta` backward: the staged forward step
    /// of the source delta it translates to, with that source delta. On a
    /// refusal the held state is as it was.
    ///
    /// # Errors
    ///
    /// The refusal of applying `delta` to the view, of put, or of the
    /// forward step, and [`ShapeEditError::PutGetMismatch`] when the forward
    /// step does not give back the edited view.
    fn stage_put(
        lens: &Lens,
        context: &Self::Context,
        held: &mut Self::Held,
        delta: &Self::Delta,
    ) -> Result<(Self::Staged, Self::Delta), ShapeEditError>;

    /// The view delta of a staged step.
    fn staged_view_delta(staged: &Self::Staged) -> &Self::Delta;

    /// Restore the held state of a staged step that is not committed.
    ///
    /// # Errors
    ///
    /// [`ShapeEditError::Incoherent`] when the inverse does not apply.
    fn abort(held: &mut Self::Held, staged: &Self::Staged) -> Result<(), ShapeEditError>;

    /// Advance the view, the complement and the indexes to a staged step;
    /// on a refusal the held state is as it was before the step.
    ///
    /// # Errors
    ///
    /// [`ShapeEditError::Incoherent`] when the view delta does not apply.
    fn commit(held: &mut Self::Held, staged: Self::Staged) -> Result<(), ShapeEditError>;

    /// The empty delta.
    fn identity() -> Self::Delta;

    /// The composite of `first` then `next`.
    ///
    /// # Errors
    ///
    /// The composition refusal of the delta algebra.
    fn compose(first: &Self::Delta, next: &Self::Delta) -> Result<Self::Delta, ComposeError>;

    /// The digest of a delta's canonical bytes.
    fn digest(delta: &Self::Delta) -> Digest;
}

/// A shape whose held source, view and complement are in-memory instances,
/// with a whole-state put and the incidence-closed replacement differ.
pub trait WholeStateShape: EditShape {
    /// The complement get returns.
    type Complement: Clone + Debug;

    /// The held source.
    fn source(held: &Self::Held) -> &Self::Source;
    /// The held view.
    fn view(held: &Self::Held) -> &Self::Source;
    /// The held complement.
    fn complement(held: &Self::Held) -> &Self::Complement;

    /// Apply `delta` to `instance`, atomically.
    ///
    /// # Errors
    ///
    /// The refusal of the delta algebra.
    fn apply(delta: &Self::Delta, instance: &mut Self::Source) -> Result<(), EditError>;

    /// Whether two instances are identical.
    fn identical(a: &Self::Source, b: &Self::Source) -> bool;

    /// The incidence-closed replacement differ from `before` to `after`
    /// keeping the replacements of `input`.
    ///
    /// # Errors
    ///
    /// [`ShapeEditError::Incoherent`] when the closed parts are malformed.
    fn closed_diff(
        before: &Self::Source,
        after: &Self::Source,
        input: &Self::Delta,
    ) -> Result<Self::Delta, ShapeEditError>;

    /// Whole-state put.
    ///
    /// # Errors
    ///
    /// The refusal of put.
    fn put(
        lens: &Lens,
        context: &Self::Context,
        view: &Self::Source,
        complement: &Self::Complement,
    ) -> Result<Self::Source, LensError>;
}

/// The backward step of a whole-state shape: put the edited view against
/// the held complement, take the incidence-closed replacement differ from
/// the held source to the put source keeping the replacements of `edit`,
/// stage its forward translation, and require it to give back the edited
/// view exactly, aborting the staged step otherwise.
///
/// # Errors
///
/// As [`EditShape::stage_put`].
pub(crate) fn stage_whole_state_put<S: WholeStateShape>(
    lens: &Lens,
    context: &S::Context,
    held: &mut S::Held,
    edit: &S::Delta,
) -> Result<(S::Staged, S::Delta), ShapeEditError>
where
    S::Source: Clone,
{
    let mut edited_view = S::view(held).clone();
    S::apply(edit, &mut edited_view).map_err(ShapeEditError::Apply)?;
    let put = S::put(lens, context, &edited_view, S::complement(held))
        .map_err(|error| ShapeEditError::Put(Box::new(error)))?;
    let source_delta = S::closed_diff(S::source(held), &put, edit)?;
    let staged = S::stage(lens, context, held, &source_delta)?;
    let mut probe = S::view(held).clone();
    let agrees = S::apply(S::staged_view_delta(&staged), &mut probe).is_ok()
        && S::identical(&probe, &edited_view);
    if !agrees {
        S::abort(held, &staged)?;
        return Err(ShapeEditError::PutGetMismatch);
    }
    Ok((staged, source_delta))
}

/// A lens bound to a source of shape `S`, translating deltas in both
/// directions.
pub struct ShapeEditLens<S: EditShape> {
    lens: Lens,
    context: S::Context,
    held: S::Held,
    sequence: u64,
}

impl<S: EditShape> Clone for ShapeEditLens<S>
where
    S::Held: Clone,
{
    fn clone(&self) -> Self {
        Self {
            lens: Lens {
                compiled: self.lens.compiled.clone(),
                src_schema: self.lens.src_schema.clone(),
                tgt_schema: self.lens.tgt_schema.clone(),
            },
            context: self.context.clone(),
            held: self.held.clone(),
            sequence: self.sequence,
        }
    }
}

impl<S: EditShape> Debug for ShapeEditLens<S>
where
    S::Held: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShapeEditLens")
            .field("lens", &self.lens)
            .field("context", &self.context)
            .field("held", &self.held)
            .field("sequence", &self.sequence)
            .finish()
    }
}

/// The composite of `edits`, applied in order.
fn compose_all<S: EditShape>(edits: &[S::Delta]) -> Result<S::Delta, ShapeEditError> {
    edits.iter().try_fold(S::identity(), |acc, edit| {
        S::compose(&acc, edit).map_err(ShapeEditError::Compose)
    })
}

impl<S: EditShape> ShapeEditLens<S> {
    /// Bind `lens` to `source` under `context`: run get on it and hold the
    /// source, the view and the stamped complement.
    ///
    /// # Errors
    ///
    /// [`ShapeEditError::Get`] with the refusal of get on `source`.
    pub fn bind(
        lens: Lens,
        context: S::Context,
        source: S::Source,
    ) -> Result<Self, ShapeEditError> {
        let held = S::bind(&lens, &context, source)?;
        Ok(Self {
            lens,
            context,
            held,
            sequence: 0,
        })
    }

    /// The lens.
    #[must_use]
    pub const fn lens(&self) -> &Lens {
        &self.lens
    }

    /// The context get and put read.
    #[must_use]
    pub const fn context(&self) -> &S::Context {
        &self.context
    }

    /// The held state.
    #[must_use]
    pub(crate) const fn held(&self) -> &S::Held {
        &self.held
    }

    /// How many edits the lens has admitted.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    /// Translate the source delta `edit` to the view, and advance the held
    /// source, view and complement.
    ///
    /// # Errors
    ///
    /// [`ShapeEditError::Apply`] when `edit` does not apply to the held
    /// source, [`ShapeEditError::Get`] when get refuses the edited source,
    /// and [`ShapeEditError::IdsExhausted`] when a new synthesized element
    /// finds no id. The lens is then unchanged.
    pub fn get_edit(&mut self, edit: &S::Delta) -> Result<ShapeEditStep<S::Delta>, ShapeEditError> {
        let staged = S::stage(&self.lens, &self.context, &mut self.held, edit)?;
        let view_delta = S::staged_view_delta(&staged).clone();
        S::commit(&mut self.held, staged)?;
        Ok(self.record(EditDirection::Get, edit, edit.clone(), view_delta))
    }

    /// Translate the view delta `edit` to the source, and advance the held
    /// source, view and complement.
    ///
    /// The shape's backward step ([`EditShape::stage_put`]) puts the edited
    /// view against the held complement and gives the source delta, whose
    /// forward translation must give back the edited view exactly; for a
    /// whole-state shape the source delta is the incidence-closed
    /// replacement differ from the held source to the put source, keeping
    /// the replacements of `edit`.
    ///
    /// # Errors
    ///
    /// [`ShapeEditError::Apply`] when `edit` does not apply to the held
    /// view, [`ShapeEditError::Put`] when put refuses the edited view,
    /// [`ShapeEditError::Get`] or [`ShapeEditError::IdsExhausted`] when the
    /// forward translation of the source delta refuses, and
    /// [`ShapeEditError::PutGetMismatch`] when it does not give back the
    /// edited view. The lens is then unchanged.
    pub fn put_edit(&mut self, edit: &S::Delta) -> Result<ShapeEditStep<S::Delta>, ShapeEditError> {
        let (staged, source_delta) = S::stage_put(&self.lens, &self.context, &mut self.held, edit)?;
        S::commit(&mut self.held, staged)?;
        Ok(self.record(EditDirection::Put, edit, source_delta, edit.clone()))
    }

    /// Translate a batch of source deltas, applied in order, as one edit.
    ///
    /// # Errors
    ///
    /// [`ShapeEditError::Compose`] when two adjacent deltas do not compose,
    /// and otherwise as [`Self::get_edit`] on the composite. No delta of the
    /// batch is admitted unless all are.
    pub fn get_batch(
        &mut self,
        edits: &[S::Delta],
    ) -> Result<ShapeEditStep<S::Delta>, ShapeEditError> {
        let composite = compose_all::<S>(edits)?;
        self.get_edit(&composite)
    }

    /// Translate a batch of view deltas, applied in order, as one edit.
    ///
    /// # Errors
    ///
    /// [`ShapeEditError::Compose`] when two adjacent deltas do not compose,
    /// and otherwise as [`Self::put_edit`] on the composite. No delta of the
    /// batch is admitted unless all are.
    pub fn put_batch(
        &mut self,
        edits: &[S::Delta],
    ) -> Result<ShapeEditStep<S::Delta>, ShapeEditError> {
        let composite = compose_all::<S>(edits)?;
        self.put_edit(&composite)
    }

    fn record(
        &mut self,
        direction: EditDirection,
        input: &S::Delta,
        source_delta: S::Delta,
        view_delta: S::Delta,
    ) -> ShapeEditStep<S::Delta> {
        self.sequence += 1;
        ShapeEditStep {
            sequence: self.sequence,
            direction,
            input: S::digest(input),
            source_delta,
            view_delta,
        }
    }
}

impl<S: WholeStateShape> ShapeEditLens<S> {
    /// The held source.
    #[must_use]
    pub fn source(&self) -> &S::Source {
        S::source(&self.held)
    }

    /// The held view.
    #[must_use]
    pub fn view(&self) -> &S::Source {
        S::view(&self.held)
    }

    /// The held complement, stamped with the identity of the lens.
    #[must_use]
    pub fn complement(&self) -> &S::Complement {
        S::complement(&self.held)
    }
}
