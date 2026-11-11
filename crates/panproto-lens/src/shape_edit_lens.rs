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
//! The two translations are the same for every shape. A forward translation
//! is the incidence-closed replacement differ between the held view and the
//! view of the edited source, keeping the replacements of the input
//! ([`GraphDelta::diff_keeping_replacements`],
//! [`TableDelta::diff_keeping_replacements`]); a backward translation puts
//! the edited view, takes the same differ from the held source to the put
//! source, translates that delta forward, and requires the result to be the
//! edited view. A shape ([`EditShape`]) supplies its delta algebra, its
//! whole-state put and its forward step: [`GraphEditLens`] runs the graph
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
use panproto_inst::segment::IdKind;
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

/// An instance shape with a delta algebra, a whole-state put and a forward
/// edit step: what [`ShapeEditLens`] needs of a shape.
pub trait EditShape: Sized {
    /// The instances of the shape.
    type Instance: Clone + Debug;
    /// The complement get returns.
    type Complement: Clone + Debug;
    /// The deltas of the shape.
    type Delta: Clone + Debug + Eq + Serialize + DeserializeOwned;
    /// What get and put read besides the lens: the boundary protocols of a
    /// table, nothing for a graph.
    type Context: Clone + Debug;
    /// The held source, view and complement, with the shape's indexes.
    type Held: Clone + Debug;
    /// A forward step whose source change is made but not committed.
    type Staged;

    /// Run get on `source` and hold it with its view and complement.
    ///
    /// # Errors
    ///
    /// [`ShapeEditError::Get`] with the refusal of get on `source`.
    fn bind(
        lens: &Lens,
        context: &Self::Context,
        source: Self::Instance,
    ) -> Result<Self::Held, ShapeEditError>;

    /// The held source.
    fn source(held: &Self::Held) -> &Self::Instance;
    /// The held view.
    fn view(held: &Self::Held) -> &Self::Instance;
    /// The held complement.
    fn complement(held: &Self::Held) -> &Self::Complement;

    /// Apply `delta` to the held source and translate it forward, leaving
    /// the view and complement as they are; on a refusal the source is
    /// restored.
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

    /// The view delta of a staged step.
    fn staged_view_delta(staged: &Self::Staged) -> &Self::Delta;

    /// Restore the source of a staged step that is not committed.
    ///
    /// # Errors
    ///
    /// [`ShapeEditError::Incoherent`] when the inverse does not apply.
    fn abort(held: &mut Self::Held, staged: &Self::Staged) -> Result<(), ShapeEditError>;

    /// Advance the view, the complement and the indexes to a staged step;
    /// on a refusal the source is restored and nothing else changes.
    ///
    /// # Errors
    ///
    /// [`ShapeEditError::Incoherent`] when the view delta does not apply.
    fn commit(held: &mut Self::Held, staged: Self::Staged) -> Result<(), ShapeEditError>;

    /// Apply `delta` to `instance`, atomically.
    ///
    /// # Errors
    ///
    /// The refusal of the delta algebra.
    fn apply(delta: &Self::Delta, instance: &mut Self::Instance) -> Result<(), EditError>;

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

    /// Whether two instances are identical.
    fn identical(a: &Self::Instance, b: &Self::Instance) -> bool;

    /// The incidence-closed replacement differ from `before` to `after`
    /// keeping the replacements of `input`.
    ///
    /// # Errors
    ///
    /// [`ShapeEditError::Incoherent`] when the closed parts are malformed.
    fn closed_diff(
        before: &Self::Instance,
        after: &Self::Instance,
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
        view: &Self::Instance,
        complement: &Self::Complement,
    ) -> Result<Self::Instance, LensError>;
}

/// A lens bound to a source of shape `S`, translating deltas in both
/// directions.
pub struct ShapeEditLens<S: EditShape> {
    lens: Lens,
    context: S::Context,
    held: S::Held,
    sequence: u64,
}

impl<S: EditShape> Clone for ShapeEditLens<S> {
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

impl<S: EditShape> Debug for ShapeEditLens<S> {
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
        source: S::Instance,
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

    /// The held source.
    #[must_use]
    pub fn source(&self) -> &S::Instance {
        S::source(&self.held)
    }

    /// The held view.
    #[must_use]
    pub fn view(&self) -> &S::Instance {
        S::view(&self.held)
    }

    /// The held complement, stamped with the identity of the lens.
    #[must_use]
    pub fn complement(&self) -> &S::Complement {
        S::complement(&self.held)
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
    /// The edited view is put against the held complement; the source delta
    /// is the incidence-closed replacement differ from the held source to
    /// the put source, keeping the replacements of `edit`. Its forward
    /// translation must give back the edited view exactly.
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
        let mut edited_view = self.view().clone();
        S::apply(edit, &mut edited_view).map_err(ShapeEditError::Apply)?;
        let put = S::put(&self.lens, &self.context, &edited_view, self.complement())
            .map_err(|error| ShapeEditError::Put(Box::new(error)))?;
        let source_delta = S::closed_diff(self.source(), &put, edit)?;
        let staged = S::stage(&self.lens, &self.context, &mut self.held, &source_delta)?;
        let mut probe = self.view().clone();
        let agrees = S::apply(S::staged_view_delta(&staged), &mut probe).is_ok()
            && S::identical(&probe, &edited_view);
        if !agrees {
            S::abort(&mut self.held, &staged)?;
            return Err(ShapeEditError::PutGetMismatch);
        }
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
