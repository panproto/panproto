//! Collecting the artifacts of segmented runs no holder needs.
//!
//! A store accumulates the artifacts of every run: the journal and
//! committed artifacts of complete runs, of runs stopped part way, and of
//! runs whose input was superseded, and artifacts staged by a commit that
//! never happened. [`collect_garbage`] keeps exactly what the given *roots*
//! can read and removes everything else.
//!
//! The roots are run identities: a run a holder may resume (a run stopped
//! by a store failure, a cancellation or a budget), and a complete run
//! whose committed artifacts a holder reads as its result (a restriction
//! whose Emit artifacts hold a view, an edit run whose Emit artifacts hold
//! rewritten segments). An artifact is *reachable* when its run is a root
//! and it is committed: a run reads only artifacts its journal names, and
//! commit journals an artifact in the same atomic step that makes it
//! readable, so every committed artifact of a root is journaled and an
//! artifact only staged is never read. The journal of a root is reachable
//! too. A run that carries tasks over from an earlier run
//! ([`RunSpec::reuse`](super::RunSpec::reuse)) copies them, so its own
//! artifacts do not depend on the earlier run's, and collecting the earlier
//! run only makes a later run recompute what it would have carried over.
//!
//! Collection removes every journal of a run that is not a root before any
//! artifact, so a collection interrupted part way leaves no journal naming
//! a removed artifact; it never touches a reachable artifact or journal.

use std::collections::BTreeSet;

use super::store::{ArtifactKey, SegmentStore};
use crate::identity::Digest;

/// Whether a stored artifact is staged or committed. A key may be both:
/// committed, and staged again by a later attempt of its task.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Stored {
    /// Staged, not readable.
    Staged,
    /// Committed and readable.
    Committed,
}

/// A [`SegmentStore`] whose contents can be listed and removed, so that
/// [`collect_garbage`] can reclaim the artifacts no root reads.
pub trait CollectableStore: SegmentStore {
    /// Every stored artifact with whether it is staged or committed, a key
    /// both staged and committed listed once for each.
    ///
    /// # Errors
    ///
    /// The store's error when it cannot be listed.
    fn stored(&self) -> Result<Vec<(ArtifactKey, Stored)>, Self::Error>;

    /// Every run with a journal.
    ///
    /// # Errors
    ///
    /// The store's error when it cannot be listed.
    fn journaled(&self) -> Result<Vec<Digest>, Self::Error>;

    /// Remove the artifact `key` in state `stored`; removing one that is
    /// not there does nothing.
    ///
    /// # Errors
    ///
    /// The store's error when it cannot be removed.
    fn remove(&mut self, key: &ArtifactKey, stored: Stored) -> Result<(), Self::Error>;

    /// Remove the journal of `run`; removing one that is not there does
    /// nothing.
    ///
    /// # Errors
    ///
    /// The store's error when it cannot be removed.
    fn remove_journal(&mut self, run: &Digest) -> Result<(), Self::Error>;
}

/// What a collection removed and kept.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Collection {
    /// The journals removed, ascending.
    pub journals: Vec<Digest>,
    /// The artifacts removed, ascending by key and state.
    pub removed: Vec<(ArtifactKey, Stored)>,
    /// The number of artifacts kept.
    pub kept: usize,
}

/// Whether the artifact `key` in state `stored` is reachable from `roots`:
/// committed, and written by a root.
#[must_use]
pub fn reachable(roots: &BTreeSet<Digest>, key: &ArtifactKey, stored: Stored) -> bool {
    stored == Stored::Committed && roots.contains(&key.run)
}

/// Remove from `store` every journal of a run outside `roots` and every
/// artifact [`reachable`] does not admit, keeping every other journal and
/// artifact as it is.
///
/// Journals are removed first, in ascending run order, then artifacts, in
/// ascending key and state order.
///
/// # Errors
///
/// The store's first error; what was removed before it stays removed, and
/// nothing reachable was touched.
pub fn collect_garbage<St: CollectableStore>(
    store: &mut St,
    roots: &BTreeSet<Digest>,
) -> Result<Collection, St::Error> {
    let mut journals: Vec<Digest> = store
        .journaled()?
        .into_iter()
        .filter(|run| !roots.contains(run))
        .collect();
    journals.sort_unstable();
    journals.dedup();
    for run in &journals {
        store.remove_journal(run)?;
    }
    let mut stored = store.stored()?;
    stored.sort_unstable();
    stored.dedup();
    let (kept, removed): (Vec<_>, Vec<_>) = stored
        .into_iter()
        .partition(|(key, state)| reachable(roots, key, *state));
    for (key, state) in &removed {
        store.remove(key, *state)?;
    }
    Ok(Collection {
        journals,
        removed,
        kept: kept.len(),
    })
}
