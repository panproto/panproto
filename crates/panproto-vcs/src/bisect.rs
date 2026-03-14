//! Binary search for the commit that introduced a breaking change.
//!
//! Given a "good" commit (no breaking changes) and a "bad" commit
//! (has breaking changes), bisect narrows down the exact commit that
//! introduced the break.

use crate::dag;
use crate::error::VcsError;
use crate::hash::ObjectId;
use crate::store::Store;

/// State of an in-progress bisect session.
#[derive(Clone, Debug)]
pub struct BisectState {
    /// The full path from good to bad (inclusive).
    path: Vec<ObjectId>,
    /// Current low index (inclusive, known good).
    lo: usize,
    /// Current high index (inclusive, known bad).
    hi: usize,
}

/// Result of a bisect step.
#[derive(Clone, Debug)]
pub enum BisectStep {
    /// Test this commit next (the user or automated checker decides
    /// if it's good or bad).
    Test(ObjectId),
    /// The search is complete: this commit introduced the break.
    Found(ObjectId),
}

/// Start a bisect session.
///
/// Finds the path from `good` to `bad` in the DAG and returns the
/// initial state plus the first commit to test.
///
/// # Errors
///
/// Returns [`VcsError::NoPath`] if no path exists between the commits.
pub fn bisect_start(
    store: &dyn Store,
    good: ObjectId,
    bad: ObjectId,
) -> Result<(BisectState, BisectStep), VcsError> {
    let path = dag::find_path(store, good, bad)?;

    if path.len() <= 2 {
        // good and bad are adjacent — bad is the answer.
        let state = BisectState {
            path: path.clone(),
            lo: 0,
            hi: path.len().saturating_sub(1),
        };
        return Ok((state, BisectStep::Found(bad)));
    }

    let state = BisectState {
        lo: 0,
        hi: path.len() - 1,
        path,
    };

    let mid = usize::midpoint(state.lo, state.hi);
    Ok((state.clone(), BisectStep::Test(state.path[mid])))
}

/// Advance the bisect by one step.
///
/// `is_good` indicates whether the commit returned by the previous
/// [`BisectStep::Test`] was good (no breaking changes) or bad.
///
/// # Errors
///
/// Returns an error if the state is invalid.
pub fn bisect_step(state: &mut BisectState, is_good: bool) -> BisectStep {
    let mid = usize::midpoint(state.lo, state.hi);

    if is_good {
        state.lo = mid;
    } else {
        state.hi = mid;
    }

    if state.hi - state.lo <= 1 {
        // The bad commit is at hi (lo is the last known good).
        return BisectStep::Found(state.path[state.hi]);
    }

    let new_mid = usize::midpoint(state.lo, state.hi);
    BisectStep::Test(state.path[new_mid])
}

/// Returns the number of remaining steps in the bisect.
#[must_use]
pub const fn bisect_remaining(state: &BisectState) -> usize {
    let range = state.hi - state.lo;
    if range <= 1 {
        0
    } else {
        // log2(range) rounded up.
        (usize::BITS - range.leading_zeros()) as usize
    }
}

#[cfg(test)]
#[allow(clippy::cast_possible_truncation)]
mod tests {
    use super::*;
    use crate::MemStore;
    use crate::object::{CommitObject, Object};

    fn build_linear(n: usize) -> Result<(MemStore, Vec<ObjectId>), Box<dyn std::error::Error>> {
        let mut store = MemStore::new();
        let mut ids = Vec::new();

        for i in 0..n {
            let parents = if i == 0 { vec![] } else { vec![ids[i - 1]] };
            let commit = CommitObject {
                schema_id: ObjectId::from_bytes([i as u8; 32]),
                parents,
                migration_id: None,
                protocol: "test".into(),
                author: "test".into(),
                timestamp: i as u64 * 100,
                message: format!("commit {i}"),
            };
            let id = store.put(&Object::Commit(commit))?;
            ids.push(id);
        }

        Ok((store, ids))
    }

    #[test]
    fn bisect_finds_in_linear_history() -> Result<(), Box<dyn std::error::Error>> {
        let (store, ids) = build_linear(8)?;
        // Suppose commit 5 introduced the break.
        // good = ids[0], bad = ids[7].
        let (mut state, step) = bisect_start(&store, ids[0], ids[7])?;

        // Walk through bisect steps.
        let breaking_index = 5;
        let mut steps = 0;
        let mut current_step = step;

        loop {
            match current_step {
                BisectStep::Found(id) => {
                    assert_eq!(id, ids[breaking_index]);
                    break;
                }
                BisectStep::Test(id) => {
                    // Simulate: commits before index 5 are good, 5+ are bad.
                    let idx = ids
                        .iter()
                        .position(|i| *i == id)
                        .ok_or("commit not found in ids")?;
                    let is_good = idx < breaking_index;
                    current_step = bisect_step(&mut state, is_good);
                    steps += 1;
                    assert!(steps <= 10, "bisect should converge");
                }
            }
        }
        Ok(())
    }

    #[test]
    fn bisect_adjacent_commits() -> Result<(), Box<dyn std::error::Error>> {
        let (store, ids) = build_linear(2)?;
        let (_state, step) = bisect_start(&store, ids[0], ids[1])?;
        assert!(matches!(step, BisectStep::Found(id) if id == ids[1]));
        Ok(())
    }

    #[test]
    fn bisect_remaining_count() {
        let state = BisectState {
            path: vec![ObjectId::ZERO; 16],
            lo: 0,
            hi: 15,
        };
        assert!(bisect_remaining(&state) <= 4);
    }
}
